use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use hyper_util::{client::legacy::Client as HyperClient, rt::TokioExecutor};
use hyperlocal::UnixConnector;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::time::timeout;

use crate::display;

const DEFAULT_DOCKER_SOCKET: &str = "/var/run/docker.sock";
const DEFAULT_DOCKER_API_VERSION: &str = "v1.43";

pub struct DockerClient {
    client: HyperClient<UnixConnector, Full<Bytes>>,
    socket_path: PathBuf,
    base_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Container {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Names")]
    pub names: Option<Vec<String>>,
    #[serde(rename = "Image")]
    pub image: String,

    #[serde(rename = "ImageID")]
    pub image_id: String,

    #[serde(rename = "Command")]
    pub command: String,

    #[serde(rename = "Created")]
    pub created: u64,
    #[serde(rename = "State")]
    pub state: String,
    #[serde(rename = "Status")]
    pub status: String,
    #[serde(rename = "Ports")]
    pub ports: Option<Vec<Port>>,
    #[serde(rename = "Labels")]
    pub labels: Option<HashMap<String, String>>,
    #[serde(rename = "SizeRw")]
    pub size_rw: Option<u64>,
    #[serde(rename = "SizeRootFs")]
    pub size_root_fs: Option<u64>,
    #[serde(rename = "HostConfig")]
    pub host_config: Option<HostConfig>,
    #[serde(rename = "NetworkSettings")]
    pub network_settings: Option<NetworkSettings>,
    #[serde(rename = "Mounts")]
    pub mounts: Option<Vec<Mount>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Port {
    #[serde(rename = "IP")]
    pub ip: Option<String>,

    #[serde(rename = "PrivatePort")]
    pub private_port: u16,
    #[serde(rename = "PublicPort")]
    pub pub_port: Option<u16>,
    #[serde(rename = "Type")]
    pub port_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HostConfig {
    #[serde(rename = "NetworkMode")]
    pub network_mode: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkSettings {
    #[serde(rename = "IPAddress")]
    pub networks: Option<HashMap<String, NetworkInfo>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkInfo {
    #[serde(rename = "IPAddress")]
    pub ip_address: String,
    #[serde(rename = "Gateway")]
    pub gateway: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Mount {
    #[serde(rename = "Type")]
    pub mount_type: String,
    #[serde(rename = "Source")]
    pub source: String,
    #[serde(rename = "Destination")]
    pub destination: String,
    #[serde(rename = "Mode")]
    pub mode: String,
    #[serde(rename = "RW")]
    pub rw: bool,
}

#[derive(Debug, Deserialize)]
pub struct ContainerStats {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "cpu_stats")]
    pub cpu_stats: CpuStats,
    #[serde(rename = "precpu_stats")]
    pub precpu_stats: CpuStats,
    #[serde(rename = "memory_stats")]
    pub memory_stats: MemoryStats,
    #[serde(rename = "networks")]
    pub networks: Option<HashMap<String, NetworkStats>>,
}

#[derive(Debug, Deserialize)]
pub struct CpuStats {
    #[serde(rename = "cpu_usage")]
    pub cpu_usage: CpuUsage,
    #[serde(rename = "system_cpu_usage")]
    pub system_cpu_usage: Option<u64>,
    #[serde(rename = "online_cpus")]
    pub online_cpus: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct CpuUsage {
    #[serde(rename = "total_usage")]
    pub total_usage: u64,
}

#[derive(Debug, Deserialize)]
pub struct MemoryStats {
    #[serde(rename = "usage")]
    pub usage: Option<u64>,
    #[serde(rename = "limit")]
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct NetworkStats {
    #[serde(rename = "rx_bytes")]
    pub rx_bytes: u64,
    #[serde(rename = "tx_bytes")]
    pub tx_bytes: u64,
}

impl DockerClient {
    pub fn new(socket_path: Option<&Path>) -> Result<Self> {
        let socket_path = match socket_path {
            Some(path) => path.to_path_buf(),
            None => PathBuf::from(DEFAULT_DOCKER_SOCKET),
        };

        // verify that the socket exists
        if !socket_path.exists() {
            let error_msg = format!("docker socket not found at {}", socket_path.display());
            error!("{error_msg}");
            return Err(anyhow::anyhow!(error_msg));
        }

        // create unix domain socket connector
        let connector = UnixConnector;
        let client = HyperClient::builder(TokioExecutor::new()).build(connector);

        let base_url = format!("http://unix/{}", DEFAULT_DOCKER_API_VERSION);
        debug!(
            "init docker client with socket path: {}",
            socket_path.display()
        );

        Ok(DockerClient {
            client,
            socket_path,
            base_url,
        })
    }

    async fn make_request<T: for<'de> Deserialize<'de>>(
        &self,
        method: hyper::Method,
        path: &str,
        query: Option<&[(&str, &str)]>,
        body: Option<Value>,
    ) -> Result<T> {
        debug!("making {method} request to {path}");

        let mut full_path = path.to_string();

        // add query parameters
        if let Some(params) = query {
            let mut separator = '?';
            for (key, value) in params {
                full_path.push(separator);
                full_path.push_str(&format!(
                    "{}={}",
                    urlencoding::encode(key),
                    urlencoding::encode(value),
                ));
                separator = '&';
            }
        }

        let uri = hyperlocal::Uri::new(&self.socket_path, &full_path);

        let mut req_builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("Host", "localhost");

        if body.is_some() {
            req_builder = req_builder.header("Content-Type", "application/json");
        }

        let request = match body {
            Some(b) => {
                let json_bytes = serde_json::to_vec(&b)?;
                req_builder.body(Full::new(Bytes::from(json_bytes)))?
            }
            None => req_builder.body(Full::new(Bytes::new()))?,
        };

        let response = timeout(Duration::from_secs(30), self.client.request(request))
            .await
            .context("request timed out")?
            .context("failed to send request")?;

        let status = response.status();
        let body_bytes = response.into_body().collect().await?.to_bytes();

        if !status.is_success() {
            let error_msg = String::from_utf8_lossy(&body_bytes);
            error!("docker API error: {status} - {error_msg}");
            return Err(anyhow::anyhow!(
                "docker API error: {} - {}",
                status,
                error_msg
            ));
        }

        let parsed: T = serde_json::from_slice(&body_bytes)
            .with_context(|| format!("failed to parse JSON response from {path}"))?;

        Ok(parsed)
    }

    pub async fn list_containers(&self, all: bool, quiet: bool, format: String) -> Result<()> {
        let all_str = if all { "true" } else { "false" };
        let query = &[("all", all_str)];

        let containers: Vec<Container> = self
            .make_request(hyper::Method::GET, "/containers/json", Some(query), None)
            .await?;

        match format.as_str() {
            "json" => {
                if quiet {
                    let ids: Vec<&str> = containers.iter().map(|c| c.id.as_str()).collect();
                    println!("{}", serde_json::to_string_pretty(&ids)?);
                } else {
                    println!("{}", serde_json::to_string_pretty(&containers)?);
                }
            }
            _ => {
                display::print_containers_table(&containers, quiet)?;
            }
        }

        Ok(())
    }

    pub async fn inspect_containers(&self, ids: &[String]) -> Result<()> {
        for id in ids {
            match self.inspect_container(id).await {
                Ok(container_info) => {
                    println!("{}", serde_json::to_string_pretty(&container_info)?);
                }
                Err(e) => {
                    error!("failed to inspect container '{id}': {e}");
                    eprintln!("error: failed to inspect container '{id}': {e}");
                }
            }
        }

        Ok(())
    }

    async fn inspect_container(&self, id: &str) -> Result<Value> {
        self.make_request(
            hyper::Method::GET,
            &format!("/containers/{id}/json"),
            None,
            None,
        )
        .await
    }

    pub async fn start_containers(&self, ids: &[String]) -> Result<()> {
        for id in ids {
            match self.start_container(id).await {
                Ok(_) => {
                    info!("container '{id}' started successfully");
                    println!("container '{id}' started successfully");
                }
                Err(e) => {
                    error!("failed to start container '{id}': {e}");
                    eprintln!("failed to start container '{id}': {e}");
                }
            }
        }
        Ok(())
    }

    async fn start_container(&self, id: &str) -> Result<()> {
        let url = format!("/containers/{id}/start");
        let _: Option<Value> = self
            .make_request(hyper::Method::POST, &url, None, None)
            .await?;

        Ok(())
    }

    pub async fn stop_containers(&self, ids: &[String], timeout_secs: Option<u64>) -> Result<()> {
        for id in ids {
            let query = timeout_secs.map(|t| vec![("t", t.to_string())]);

            let query_refs: Option<Vec<(&str, &str)>> = query
                .as_ref()
                .map(|q| q.iter().map(|(k, v)| (k.as_ref(), v.as_ref())).collect());

            match self.stop_container(id, query_refs.as_deref()).await {
                Ok(_) => {
                    info!("container '{id}' stopped successfully");
                    println!("container '{id}' stopped successfully");
                }

                Err(e) => {
                    error!("failed to stopped container '{id}': {e}");
                    eprintln!("failed to stopped container '{id}': {e}");
                }
            }
        }

        Ok(())
    }

    async fn stop_container(&self, id: &str, query: Option<&[(&str, &str)]>) -> Result<()> {
        let url = format!("/containers/{id}/stop");
        let _: Option<Value> = self
            .make_request(hyper::Method::POST, &url, query, None)
            .await?;

        Ok(())
    }

    pub async fn restart_containers(
        &self,
        ids: &[String],
        timeout_secs: Option<u64>,
    ) -> Result<()> {
        for id in ids {
            let query = timeout_secs.map(|t| vec![("t", t.to_string())]);

            let query_refs: Option<Vec<(&str, &str)>> = query
                .as_ref()
                .map(|q| q.iter().map(|(k, v)| (k.as_ref(), v.as_ref())).collect());

            match self.restart_container(id, query_refs.as_deref()).await {
                Ok(_) => {
                    info!("container '{id}' restarted successfully");
                    println!("container '{id}' restarted successfully");
                }
                Err(e) => {
                    error!("failed to restarted container '{id}': {e}");
                    eprintln!("failed to restarted container '{id}': {e}");
                }
            }
        }

        Ok(())
    }

    async fn restart_container(&self, id: &str, query: Option<&[(&str, &str)]>) -> Result<()> {
        let url = format!("/containers/{id}/restart");
        let _: Option<Value> = self
            .make_request(hyper::Method::POST, &url, query, None)
            .await?;

        Ok(())
    }

    pub async fn remove_containers(
        &self,
        ids: &[String],
        force: bool,
        volumes: bool,
    ) -> Result<()> {
        let force_str = if force { "true" } else { "false" };
        let volumes_str = if volumes { "true" } else { "false" };

        let query = &[("force", force_str), ("v", volumes_str)];

        for id in ids {
            match self.remove_container(id, Some(query)).await {
                Ok(_) => {
                    info!("container '{id}' removed successfully");
                    println!("container '{id}' removed successfully");
                }
                Err(e) => {
                    error!("failed to removed container '{id}': {e}");
                    eprintln!("failed to removed container '{id}': {e}");
                }
            }
        }

        Ok(())
    }

    async fn remove_container(&self, id: &str, query: Option<&[(&str, &str)]>) -> Result<()> {
        let url = format!("/containers/{id}");
        let _: Option<Value> = self
            .make_request(hyper::Method::DELETE, &url, query, None)
            .await?;

        Ok(())
    }

    pub async fn container_stats(&self, ids: &[String]) -> Result<()> {
        let containers = if ids.is_empty() {
            let all_containers: Vec<Container> = self
                .make_request(
                    hyper::Method::GET,
                    "/containers/json",
                    Some(&[("all", "false")]),
                    None,
                )
                .await?;

            all_containers.into_iter().map(|c| c.id).collect()
        } else {
            ids.to_vec()
        };

        display::print_stats_header();

        for id in containers {
            match self.get_container_stats(&id).await {
                Ok(stats) => {
                    display::print_container_stats(&stats)?;
                }
                Err(e) => {
                    error!("failed to get stats for container '{id}': {e}");
                    eprintln!("failed to get stats for container '{id}': {e}");
                }
            }
        }
        Ok(())
    }

    async fn get_container_stats(&self, id: &str) -> Result<ContainerStats> {
        let url = format!("/containers/{id}/stats");
        let query = &[("stream", "false")];

        self.make_request(hyper::Method::GET, &url, Some(query), None)
            .await
    }

    pub async fn container_logs(
        &self,
        id: &str,
        follow: bool,
        tail: Option<String>,
        since: Option<String>,
        until: Option<String>,
        timestamps: bool,
    ) -> Result<()> {
        let follow_str = if follow { "true" } else { "false" };
        let timestamps_str = if timestamps { "true" } else { "false" };

        let mut query_params = vec![
            ("follow", follow_str),
            ("stdout", "true"),
            ("stderr", "true"),
            ("timestamps", timestamps_str),
        ];

        // build query param
        let tail_string;
        let since_string;
        let until_string;

        if let Some(t) = &tail {
            tail_string = t.clone();
            query_params.push(("tail", &tail_string));
        }

        if let Some(s) = &since {
            since_string = s.clone();
            query_params.push(("since", &since_string));
        }

        if let Some(u) = &until {
            until_string = u.clone();
            query_params.push(("until", &until_string));
        }

        let uri = hyperlocal::Uri::new(&self.socket_path, &format!("/containers/{id}/logs"));

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Host", "localhost")
            .body(Full::new(Bytes::new()))?;

        // send the req
        let response = self.client.request(request).await?;
        let status = response.status();

        if !status.is_success() {
            let body_bytes = response.into_body().collect().await?.to_bytes();
            let error_msg = String::from_utf8_lossy(&body_bytes);

            return Err(anyhow!("docker api error: {} - {}", status, error_msg));
        }

        // process streaming res
        let mut stream = response.into_body();

        while let Some(data_result) = stream.frame().await {
            let frame = data_result?;

            if let Some(data) = frame.data_ref() {
                if data.len() >= 8 {
                    let stream_type = data[0];

                    let size = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;

                    if data.len() >= 8 + size {
                        let content = &data[8..8 + size];
                        let text = String::from_utf8_lossy(content);

                        match stream_type {
                            1 => print!("{text}"), // stdout
                            2 => print!("{text}"), // stderr
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
