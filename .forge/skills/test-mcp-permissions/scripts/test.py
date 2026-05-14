#!/usr/bin/env python3
"""End-to-end tests for MCP server permission policy (PR #3324)."""

import contextlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
import time

try:
    import pexpect
except ImportError:
    subprocess.check_call([sys.executable, "-m", "pip", "install", "pexpect"])
    import pexpect

try:
    import yaml
except ImportError:
    subprocess.check_call([sys.executable, "-m", "pip", "install", "pyyaml"])
    import yaml

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
FORGE_BIN = os.path.abspath(os.environ.get("FORGE_BIN", "forge"))
PASS = "\033[32mPASS\033[0m"
FAIL = "\033[31mFAIL\033[0m"
results: list = []


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

@contextlib.contextmanager
def scenario_dirs():
    """Yield (cwd, forge_config) as isolated temp dirs, cleaned up on exit."""
    cwd = tempfile.mkdtemp(prefix="forge_mcp_cwd_")
    cfg = tempfile.mkdtemp(prefix="forge_mcp_cfg_")
    # Seed config so forge skips first-time setup
    for base in [os.path.expanduser("~/forge"), os.path.expanduser("~/.forge")]:
        if os.path.isdir(base):
            for name in [".forge.toml", ".config.json", ".credentials.json"]:
                src = os.path.join(base, name)
                if os.path.exists(src):
                    shutil.copy2(src, os.path.join(cfg, name))
            break
    try:
        yield cwd, cfg
    finally:
        shutil.rmtree(cwd, ignore_errors=True)
        shutil.rmtree(cfg, ignore_errors=True)


def perm_path(cfg: str) -> str:
    return os.path.join(cfg, "permissions.yaml")


def read_perms(cfg: str) -> dict:
    p = perm_path(cfg)
    return yaml.safe_load(open(p)) or {} if os.path.exists(p) else {}


def write_perms(cfg: str, data: dict):
    with open(perm_path(cfg), "w") as f:
        yaml.dump(data, f, default_flow_style=False)


def write_mcp(cwd: str, command: str, args=None, *, key="test-server"):
    server: dict = {"command": command}
    if args:
        server["args"] = args
    with open(os.path.join(cwd, ".mcp.json"), "w") as f:
        json.dump({"mcpServers": {key: server}}, f)


def spawn(cwd: str, cfg: str, timeout: int = 30) -> pexpect.spawn:
    env = {**os.environ, "TERM": "xterm-256color", "COLUMNS": "120", "LINES": "40", "FORGE_CONFIG": cfg}
    return pexpect.spawn(
        "/bin/sh", args=["-c", f"exec {FORGE_BIN} -p hello 2>&1"],
        cwd=cwd, timeout=timeout, encoding="utf-8", codec_errors="replace", env=env,
    )


def accept(child: pexpect.spawn):
    """Wait for the MCP permission prompt and press Enter (Accept)."""
    child.expect("Allow MCP server", timeout=30)
    child.send("\r")
    time.sleep(4)
    child.close(force=True)


def reject(child: pexpect.spawn):
    """Wait for the MCP permission prompt and press Down+Enter (Reject)."""
    child.expect("Allow MCP server", timeout=30)
    for ch in ("\x1b", "[", "B"):   # arrow-down as individual bytes for raw-mode TUI
        child.send(ch)
        time.sleep(0.1)
    time.sleep(0.4)
    child.send("\r")
    time.sleep(4)
    child.close(force=True)


def no_prompt(child: pexpect.spawn) -> bool:
    """Return True if forge exits without showing the MCP permission prompt."""
    idx = child.expect(["Allow MCP server", pexpect.TIMEOUT, pexpect.EOF], timeout=15)
    child.close(force=True)
    return idx != 0


def show_perms(cfg: str, before: dict, after: dict):
    def dump(d):
        if not d:
            print("    (empty — no permissions.yaml)")
            return
        for line in yaml.dump(d, default_flow_style=False, sort_keys=False).splitlines():
            print(f"    {line}")
    print("  ┌─ before ─────────────────────────────────")
    dump(before)
    print("  ├─ after ──────────────────────────────────")
    dump(after)
    print("  └──────────────────────────────────────────")


def run(name: str, fn):
    print(f"\n{'─'*60}\nScenario: {name}\n{'─'*60}")
    try:
        fn()
        print(f"Result: {PASS}")
        results.append((name, True, None))
    except Exception as e:
        print(f"Result: {FAIL} — {e}")
        results.append((name, False, str(e)))


def mcp_rules(perms: dict, permission: str) -> list:
    return [
        p for p in perms.get("policies", [])
        if isinstance(p, dict) and p.get("permission") == permission
        and isinstance(p.get("rule"), dict) and "mcp" in p["rule"]
    ]


# ---------------------------------------------------------------------------
# Scenarios
# ---------------------------------------------------------------------------

def test_accept_writes_allow():
    with scenario_dirs() as (cwd, cfg):
        write_mcp(cwd, "echo", ["hello"])
        before = read_perms(cfg)
        accept(spawn(cwd, cfg))
        after = read_perms(cfg)
        show_perms(cfg, before, after)
        assert mcp_rules(after, "allow"), "Expected an MCP allow rule in permissions.yaml"


def test_reject_writes_deny():
    with scenario_dirs() as (cwd, cfg):
        write_mcp(cwd, "echo", ["hello"])
        before = read_perms(cfg)
        reject(spawn(cwd, cfg))
        after = read_perms(cfg)
        show_perms(cfg, before, after)
        assert mcp_rules(after, "deny"), "Expected an MCP deny rule in permissions.yaml"


def test_existing_allow_skips_prompt():
    with scenario_dirs() as (cwd, cfg):
        write_perms(cfg, {"policies": [{"permission": "allow", "rule": {"mcp": {"command": "echo"}}}]})
        write_mcp(cwd, "echo", ["hello"])
        before = read_perms(cfg)
        assert no_prompt(spawn(cwd, cfg)), "Prompt appeared even though allow rule was pre-written"
        after = read_perms(cfg)
        show_perms(cfg, before, after)
        print("  No permission prompt shown — correct.")


def test_second_run_skips_prompt():
    with scenario_dirs() as (cwd, cfg):
        write_mcp(cwd, "echo", ["hello"])

        before1 = read_perms(cfg)
        accept(spawn(cwd, cfg))
        after1 = read_perms(cfg)
        print("  [run 1]")
        show_perms(cfg, before1, after1)
        assert os.path.exists(perm_path(cfg)), "permissions.yaml not created after first accept"

        before2 = read_perms(cfg)
        assert no_prompt(spawn(cwd, cfg)), "Prompt appeared on second run — decision was not persisted"
        after2 = read_perms(cfg)
        print("  [run 2]")
        show_perms(cfg, before2, after2)
        print("  No prompt on second run — decision persisted correctly.")


def test_npx_server_accept():
    with scenario_dirs() as (cwd, cfg):
        write_mcp(cwd, "npx", ["-y", "@modelcontextprotocol/server-filesystem", cwd], key="filesystem")
        before = read_perms(cfg)
        accept(spawn(cwd, cfg))
        after = read_perms(cfg)
        show_perms(cfg, before, after)
        rules = mcp_rules(after, "allow")
        assert rules, "Expected an MCP allow rule"
        assert rules[0]["rule"]["mcp"].get("command") == "npx", "Expected command='npx'"


def test_npx_server_reject():
    with scenario_dirs() as (cwd, cfg):
        write_mcp(cwd, "npx", ["-y", "@modelcontextprotocol/server-filesystem", cwd], key="filesystem")
        before = read_perms(cfg)
        reject(spawn(cwd, cfg))
        after = read_perms(cfg)
        show_perms(cfg, before, after)
        rules = mcp_rules(after, "deny")
        assert rules, "Expected an MCP deny rule"
        assert rules[0]["rule"]["mcp"].get("command") == "npx", "Expected command='npx'"


def test_user_scope_never_prompts():
    with scenario_dirs() as (cwd, cfg):
        # User-scope MCP lives inside FORGE_CONFIG, not in cwd
        with open(os.path.join(cfg, ".mcp.json"), "w") as f:
            json.dump({"mcpServers": {"user-server": {"command": "echo", "args": ["user-scope"]}}}, f)
        before = read_perms(cfg)
        assert no_prompt(spawn(cwd, cfg)), "Prompt appeared for a user-scope server — should be trusted unconditionally"
        after = read_perms(cfg)
        show_perms(cfg, before, after)
        print("  No prompt for user-scope server — trusted unconditionally.")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("=" * 60)
    print("MCP Permission Policy — End-to-End Tests")
    print("=" * 60)
    print(f"  forge binary: {FORGE_BIN}")

    run("Accept → allow rule written to permissions.yaml",      test_accept_writes_allow)
    run("Reject → deny rule written to permissions.yaml",       test_reject_writes_deny)
    run("Pre-existing allow rule → no prompt",                  test_existing_allow_skips_prompt)
    run("Second run after accept → no prompt",                  test_second_run_skips_prompt)
    run("Custom MCP server (npx) Accept → allow rule",          test_npx_server_accept)
    run("Custom MCP server (npx) Reject → deny rule",           test_npx_server_reject)
    run("User-scope server → never prompts",                    test_user_scope_never_prompts)

    print(f"\n{'='*60}\nSUMMARY\n{'='*60}")
    passed = sum(1 for _, ok, _ in results if ok)
    for name, ok, err in results:
        print(f"  {PASS if ok else FAIL}  {name}")
        if err:
            for line in textwrap.wrap(err, width=72):
                print(f"         {line}")
    print(f"\n{passed}/{len(results)} passed")
    sys.exit(0 if passed == len(results) else 1)


if __name__ == "__main__":
    main()
