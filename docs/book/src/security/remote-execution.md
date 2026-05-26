# Remote Execution (SSH Backend)

ZeroClaw supports running execution tools (such as shell commands) on a remote server over SSH instead of executing them directly on the host computer.

## Value

Running commands locally exposes the host computer to potential risk if the agent attempts destructive operations or makes structural system changes. By routing execution to a remote staging server, day-to-day work remains sandboxed and isolated from the developer's local workstation.

## Configuration

To route execution over SSH, configure the `[runtime]` type and the corresponding target parameters in `~/.zeroclaw/config.toml`:

```toml
[runtime]
kind = "ssh"

[runtime.ssh]
host          = "staging.internal.net"
port          = 22                         # Optional (default: 22)
username      = "deploy"
key_path      = "~/.ssh/id_ed25519"        # Optional private key file
workspace_dir = "/var/www/zeroclaw"        # Optional remote workspace directory to cd into
extra_args    = ["-o", "ConnectTimeout=5"] # Optional extra CLI args passed to ssh command
```

## How It Works

When `runtime.kind` is set to `"ssh"`, the runtime intercepts shell execution tools and routes them through a subprocess running the local `ssh` client binary.

Specifically:
1. An SSH command is formatted using settings from `[runtime.ssh]`:
   ```bash
   ssh -p <port> -o BatchMode=yes -o StrictHostKeyChecking=accept-new [-i <key_path>] [extra_args...] <username>@<host> "<remote_command>"
   ```
2. If `workspace_dir` is specified, it is automatically prepended to the command to change directories before executing the agent's command:
   ```bash
   cd <workspace_dir> && <agent_command>
   ```
3. BatchMode and StrictHostKeyChecking options are automatically injected to run non-interactively without prompting for host key verification or passwords.
