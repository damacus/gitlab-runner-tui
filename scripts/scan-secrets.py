"""Scan tracked and new non-ignored files, excluding caches and removed files."""

from pathlib import Path
import shutil
import subprocess
import tempfile

files = subprocess.check_output(
    ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"]
)
with tempfile.TemporaryDirectory(prefix="gitlab-runner-tui-secrets-") as directory:
    for entry in files.split(b"\0"):
        if not entry:
            continue
        source = Path(entry.decode())
        if source.is_symlink():
            raise SystemExit(f"Refusing to follow symlink during secret scan: {source}")
        if source.is_file():
            destination = Path(directory) / source
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
    raise SystemExit(
        subprocess.call(
            ["gitleaks", "dir", "--redact=100", "--no-banner", "--no-color", directory]
        )
    )
