pub mod docker;
pub mod native;
pub mod ssh;

pub use docker::DockerRuntime;
pub use native::NativeRuntime;
pub use ssh::SshRuntime;
pub use zeroclaw_api::runtime_traits::RuntimeAdapter;

use crate::schema::RuntimeConfig;

pub fn create_runtime(config: &RuntimeConfig) -> anyhow::Result<Box<dyn RuntimeAdapter>> {
    match config.kind.as_str() {
        "native" => Ok(Box::new(NativeRuntime::new())),
        "docker" => Ok(Box::new(DockerRuntime::new(config.docker.clone()))),
        "ssh" => Ok(Box::new(SshRuntime::new(config.ssh.clone()))),
        "cloudflare" => anyhow::bail!(
            "runtime.kind='cloudflare' is not implemented yet. Use runtime.kind='native' for now."
        ),
        other if other.trim().is_empty() => {
            anyhow::bail!("runtime.kind cannot be empty. Supported values: native, docker, ssh")
        }
        other => {
            anyhow::bail!("Unknown runtime kind '{other}'. Supported values: native, docker, ssh")
        }
    }
}
