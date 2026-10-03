#!/usr/bin/env python3

import argparse
import os
import shutil
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
if not (root / "Cargo.lock").exists():
    raise RuntimeError(str(root))


def env_bool(name: str) -> bool:
    v = os.environ.get(name)
    if v is None:
        return False
    return v.lower() not in ("false", "0")


def exec_process(command: str, args: list, cwd: Path | None = None):
    resolved_cmd = command if os.path.isabs(command) else shutil.which(command)
    if resolved_cmd is None:
        raise FileNotFoundError(command)

    if cwd:
        os.chdir(cwd)

    print(f"exec [CWD: {root}]: '{command} {' '.join(args)}'")

    os.execve(resolved_cmd, [resolved_cmd, *args], os.environ)


def cargo_build(ws: bool):
    features = ["--features=ws"] if ws or env_bool("USE_WS") else []

    print("Building dev server... (cold builds may take a while)")
    exec_process("cargo", ["build", *features, "--bin=trail"], root)


def cargo_run(port: str, ws: bool, runtime_threads: str):
    if not port:
        print("--port argument is required", file=sys.stderr)
        sys.exit(1)

    features = ["--features=ws"] if ws or env_bool("USE_WS") else []

    print("Starting dev server...")
    address = f"127.0.0.1:{port}"

    exec_process("cargo", [
        "run",
        "--bin=trail",
        *features,
        "--",
        f"--depot={root}/client/testfixture",
        f"--public-url=http://{address}",
        "run",
        f"--address={address}",
        f"--runtime-threads={runtime_threads}",
    ], root)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["build", "run"])
    parser.add_argument("--port", type=str)
    parser.add_argument("--ws", action="store_true", default=False)
    parser.add_argument("--runtime-threads", type=str, default="1")
    args = parser.parse_args()

    if args.command == "build":
        cargo_build(args.ws)
    elif args.command == "run":
        cargo_run(args.port, args.ws, args.runtime_threads)


if __name__ == "__main__":
    main()
