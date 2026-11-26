#!/usr/bin/env python3
import errno
import json
import os
import pty
import re
import select
import shlex
import shutil
import signal
import sys
import tempfile
import termios
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
QUESTION = b"Select AWS Profile:"
LIMIT = 15


def script(shell, action):
    persistent = action.startswith("persist")
    cancelled = "escape" in action or "interrupt" in action
    command = "awsps" if persistent else "awsps --current --"
    if shell == "bash":
        return f"""
source {shlex.quote(str(ROOT / "shell/awsps.bash"))}
awsps --activate fixture-a
awsps --current --activate fixture-a
code=0
{command} || code=$?
scope=saved
case "${{AWS_PROFILE_CURRENT_SHELL-}}" in true|1|yes) scope=shell ;; esac
printf '\\nRESULT|%s|%s|%s\\n' "$code" "${{AWS_PROFILE-}}" "$scope"
"""
    attempt = (
        f"let failed = (try {{{command}; false}} catch {{true}})\n"
        'if not $failed {error make {msg:"Cancellation reported success"}}\nlet code = 1'
        if cancelled
        else f"{command}\nlet code = 0"
    )
    return f"""
source {json.dumps(str(ROOT / "shell/awsps.nu"))}
awsps --activate fixture-a
awsps --current --activate fixture-a
{attempt}
let scope = if $env.AWS_PROFILE_CURRENT_SHELL? in [true "true" 1 "1" "yes"] {{ "shell" }} else {{ "saved" }}
print $"RESULT|($code)|($env.AWS_PROFILE? | default '')|($scope)"
"""


def stop(pid):
    try:
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        os.waitpid(pid, 0)
    except ChildProcessError:
        pass


def drive(argv, env, keys):
    pid, master = pty.fork()
    if pid == 0:
        termios.tcsetwinsize(0, (30, 110))
        os.execvpe(argv[0], argv, env)
    data = bytearray()
    cursor = 0
    sent = False
    status = None
    deadline = time.monotonic() + LIMIT
    try:
        while time.monotonic() < deadline:
            readable, _, _ = select.select(
                [master], [], [], min(0.1, max(0, deadline - time.monotonic()))
            )
            if readable:
                try:
                    chunk = os.read(master, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    chunk = b""
                data.extend(chunk)
                while True:
                    query = data.find(b"\x1b[6n", cursor)
                    if query == -1:
                        break
                    os.write(master, b"\x1b[1;1R")
                    cursor = query + 4
                if not sent and QUESTION in data:
                    os.write(master, keys)
                    sent = True
            waited, result = os.waitpid(pid, os.WNOHANG)
            if waited:
                status = result
                while select.select([master], [], [], 0)[0]:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                        break
                    if not chunk:
                        break
                    data.extend(chunk)
                break
        if status is None:
            raise AssertionError("Interactive selector exceeded its 15-second deadline")
        if not sent:
            raise AssertionError("Interactive menu was not visible before input")
        if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0:
            raise AssertionError("Shell did not finish successfully")
        return bytes(data)
    except BaseException as error:
        raise AssertionError(f"{error}\n{bytes(data[-8192:]).decode(errors='replace')}") from error
    finally:
        if status is None:
            stop(pid)
        os.close(master)


def run_case(binary, shell, action):
    with tempfile.TemporaryDirectory(prefix="awsps-interactive-") as directory:
        base = Path(directory)
        home = base / "home"
        state = home / ".aws"
        commands = base / "bin"
        state.mkdir(parents=True)
        commands.mkdir()
        (commands / "aws-profile-selector").symlink_to(binary)
        config = (
            "[profile fixture-a]\nsso_account_id=111111111111\nsso_role_name=Developer\n"
            "[profile fixture-b]\nsso_account_id=111111111111\nsso_role_name=DatabaseReader\n"
        )
        (state / "config").write_text(config)
        (state / "credentials").write_text("")
        current = state / "current-profile"
        current.write_text("fixture-a")
        source = base / ("case.sh" if shell == "bash" else "case.nu")
        source.write_text(script(shell, action))
        executable = shutil.which(shell)
        if executable is None:
            raise AssertionError(f"Required shell is missing: {shell}")
        argv = (
            [executable, "--noprofile", "--norc", str(source)]
            if shell == "bash"
            else [executable, "--no-config-file", str(source)]
        )
        env = {
            "PATH": str(commands) + os.pathsep + os.environ["PATH"],
            "HOME": str(home),
            "AWS_CONFIG_FILE": str(state / "config"),
            "AWS_SHARED_CREDENTIALS_FILE": str(state / "credentials"),
            "XDG_CONFIG_HOME": str(home / "config"),
            "XDG_DATA_HOME": str(home / "data"),
            "XDG_CACHE_HOME": str(home / "cache"),
            "TERM": "xterm-256color",
            "SHELL": "/bin/bash",
            "LANG": "C.UTF-8",
        }
        keys = b"\x1b" if "escape" in action else b"\x03" if "interrupt" in action else b"fixture-b\r"
        data = drive(argv, env, keys)
        match = re.search(rb"RESULT\|(\d+)\|([^|\r\n]*)\|([^|\r\n]*)", data)
        if match is None:
            raise AssertionError(f"No result record: {data.decode(errors='replace')}")
        actual = tuple(item.decode() for item in match.groups())
        cancelled = "escape" in action or "interrupt" in action
        expected = (
            ("1", "fixture-a", "shell")
            if cancelled
            else ("0", "fixture-b", "saved" if action.startswith("persist") else "shell")
        )
        if actual != expected:
            raise AssertionError(f"{shell}/{action}: incorrect shell snapshot")
        saved = "fixture-b" if action == "persist" else "fixture-a"
        if current.read_text() != saved or (state / "config").read_text() != config:
            raise AssertionError(f"{shell}/{action}: incorrect persisted metadata")
        print(f"ok - {shell}: visible menu, {action}, scope and persistence")


def main():
    binary = Path(sys.argv[1]).resolve(strict=True)
    for shell in ("bash", "nu"):
        for action in ("current", "escape", "interrupt", "persist", "persist-escape", "persist-interrupt"):
            run_case(binary, shell, action)
    print("12 bounded real-PTY AWS selection and cancellation checks passed")


if __name__ == "__main__":
    main()
