#!/usr/bin/env python3
"""Run one cargo command through the host GPU runner and preserve its status."""
import fcntl
import json
from pathlib import Path
import subprocess
import sys
import time

worktree = Path(__file__).resolve().parents[1]
identity = "/home/charles/repos/loki-wt/ids/loki-w9"
bus = ["agentbus", "--cwd", identity]
# One inbox consumer per worktree: overlapping helper processes must not
# consume and discard each other's RESULT messages.
mailbox_lock = worktree / "target/w9-validation/runner-inbox.lock"
mailbox_lock.parent.mkdir(parents=True, exist_ok=True)
mailbox_guard = mailbox_lock.open("a")
fcntl.flock(mailbox_guard, fcntl.LOCK_EX)
# Keep heavy proof benchmarks and subsequent GPU proof requests sequential.
marker = worktree / "target/w9-validation/cpu-benchmark.running"
while marker.exists():
    time.sleep(2)
tag = "w9-gate-" + str(time.time_ns())
request = {"worktree": str(worktree), "args": sys.argv[1:]}
subprocess.run(bus + ["send", "gpu-runner", "RUN " + tag,
                      "--body", json.dumps(request)], check=True)
while True:
    # Nonblocking inbox reads let the operator check coordinator messages
    # between polls without competing MQTT wait clients for this identity.
    reply = subprocess.run(bus + ["recv", "--from", "gpu-runner"],
                           text=True, capture_output=True)
    reply.check_returncode()
    matched = None
    for line in reply.stdout.splitlines():
        message = json.loads(line)
        if message["subj"] != "RESULT " + tag:
            print("Other runner response: " + line, flush=True)
            continue
        matched = json.loads(message["body"])
    if matched is not None:
        print(json.dumps(matched), flush=True)
        print(Path(matched["log"]).read_text(), end="", flush=True)
        sys.exit(matched["exit"])
    time.sleep(2)
