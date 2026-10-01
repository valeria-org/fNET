"""Validate fNET and assemble release artifacts. Never pushes or publishes.

Requires Python 3.11+, Cargo, Git, rustfmt, and clippy. No Python dependencies.
"""
import argparse
import gzip
import hashlib
import io
import json
import pathlib
import re
import shutil
import subprocess
import tarfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
REPOSITORY = "https://github.com/valeria-org/fNET"


def run(args, capture=False, env=None):
    result = subprocess.run(args, cwd=ROOT, check=True, text=True,
                            capture_output=capture, env=env)
    return result.stdout.strip() if capture else None


def metadata(tag=None, require_tag=False, allow_dirty=False):
    package = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    version = package["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise RuntimeError("Invalid release version")
    expected = {"name": "fNET", "license": "MIT", "repository": REPOSITORY,
                "homepage": REPOSITORY, "documentation": "https://docs.rs/fNET",
                "publish": ["crates-io"], "readme": "README.md"}
    for key, value in expected.items():
        if package.get(key) != value:
            raise RuntimeError(f"Release metadata {key} must equal {value!r}")
    license_text = (ROOT / "LICENSE").read_text(encoding="utf-8")
    if not license_text.startswith("MIT License\n") or "Permission is hereby granted, free of charge" not in license_text:
        raise RuntimeError("LICENSE must contain the MIT license")
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    if not any(p["name"] == "fNET" and p["version"] == version for p in lock["package"]):
        raise RuntimeError("Cargo.lock does not match the package version")
    notes = release_notes(version)
    if tag is not None and tag != f"v{version}":
        raise RuntimeError(f"Tag {tag!r} does not match v{version}")
    if not allow_dirty and run(["git", "status", "--porcelain", "--untracked-files=all"], capture=True):
        raise RuntimeError("Commit source changes first, or use --allow-dirty for local preparation")
    if require_tag:
        if tag is None or allow_dirty:
            raise RuntimeError("--require-tag requires --tag and a clean checkout")
        tagged = run(["git", "rev-parse", f"refs/tags/{tag}^{{commit}}"], capture=True)
        if tagged != run(["git", "rev-parse", "HEAD"], capture=True):
            raise RuntimeError("HEAD does not match the requested release tag")
    cargo = json.loads(run(["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"], capture=True))
    return version, pathlib.Path(cargo["target_directory"]), notes


def release_notes(version):
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    section = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## |^\[|\Z)", changelog, re.M | re.S)
    if section is None or not section[1].strip():
        raise RuntimeError(f"CHANGELOG.md has no release notes for {version}")
    return f"# fNET {version}\n\n{section[1].strip()}\n"


def verify_package(version, target, require_tag=False):
    archive = target / "package" / f"fNET-{version}.crate"
    with tarfile.open(archive, "r:gz") as package:
        prefix = f"fNET-{version}/"
        members = package.getmembers()
        if any(not member.name.startswith(prefix) or ".." in pathlib.PurePosixPath(member.name).parts for member in members):
            raise RuntimeError("Unexpected package archive path")
        files = {member.name.removeprefix(prefix) for member in members if member.isfile()}
        required = {"src/lib.rs", "src/generated.rs", "LICENSE", "NOTICE", "README.md", "CHANGELOG.md",
                    "research/MONO-AUDIT.md", "research/LICENSE-dotnet.txt", "research/LICENSE-mono.txt",
                    "research/COPYING-mono-2004.txt", "research/mono-opcode-history-audit.tsv",
                    "tests/data/runtime-corpus.bin", "tests/data/framework2/runtime-corpus.bin",
                    "tests/data/framework4/runtime-corpus.bin", "tests/data/net5/runtime-corpus.bin",
                    "tests/data/net8/runtime-corpus.bin", "tests/data/net9/runtime-corpus.bin"}
        if missing := required - files:
            raise RuntimeError(f"Package is missing files: {sorted(missing)}")
        if any(p.startswith((".git/", ".github/", "target/")) or "bin" in pathlib.PurePosixPath(p).parts
               or "obj" in pathlib.PurePosixPath(p).parts or "__pycache__" in p
               or p.endswith((".exe", ".pdb", ".dll", ".pyc"))
               or (p.startswith("research/") and p.endswith(".c")) for p in files):
            raise RuntimeError("Package includes temporary/build/reference implementation files")
        for name in files - {"Cargo.toml", ".cargo_vcs_info.json"}:
            original = "Cargo.toml" if name == "Cargo.toml.orig" else name
            path = ROOT / original
            if not path.is_file() or package.extractfile(prefix + name).read() != path.read_bytes():
                raise RuntimeError(f"Packaged {name} differs from the checkout")
        if require_tag:
            vcs = package.extractfile(prefix + ".cargo_vcs_info.json")
            if vcs is None or json.loads(vcs.read())["git"]["sha1"] != run(["git", "rev-parse", "HEAD"], capture=True):
                raise RuntimeError("Packaged source revision does not match the tagged commit")
    # Cargo already unpacked and compiled the archive during the dry run.
    # Test that copy to detect excluded fixture/support files that build-only
    # package verification would miss.
    unpacked = target / "package" / f"fNET-{version}"
    subprocess.run(["cargo", "test", "--all-features", "--locked"], cwd=unpacked, check=True)
    subprocess.run(["python", "tools/generate.py", "--check"], cwd=unpacked, check=True)
    print(f"Verified {archive.name}: {archive.stat().st_size} bytes; SHA256 {digest(archive)}")
    return archive


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package(options):
    args = (["cargo", "package", "--locked", "--offline"] if options.offline else
            ["cargo", "publish", "--dry-run", "--locked", "--registry", "crates-io"])
    if options.allow_dirty:
        args.append("--allow-dirty")
    run(args)
    if not options.offline:
        # Explicitly materialize the artifact after the registry dry run, rather
        # than trusting a .crate left by a previous packaging invocation.
        args = ["cargo", "package", "--locked"]
        if options.allow_dirty:
            args.append("--allow-dirty")
        run(args)


def check(options):
    import os
    version, target, notes = metadata(options.tag, options.require_tag, options.allow_dirty)
    run(["cargo", "fmt", "--all", "--", "--check"])
    run(["cargo", "clippy", "--all-features", "--all-targets", "--locked", "--", "-D", "warnings"])
    for flags in ([], ["--features", "alloc"], ["--all-features"]):
        run(["cargo", "test", "--locked", *flags])
    run(["python", "tools/generate.py", "--check"])
    run(["cargo", "doc", "--no-deps", "--all-features", "--locked"], env={**os.environ, "RUSTDOCFLAGS": "-D warnings"})
    package(options)
    verify_package(version, target, options.require_tag)
    print(f"fNET {version}: release checks passed")
    return version, target, notes


def artifacts(options):
    version, target, notes = check(options)
    directory = target / "dist"
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / f"fNET-{version}-source.tar.gz"
    files = run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], capture=True).split("\0")
    # Reproducible tar metadata and gzip header; source comes only from Git's
    # tracked/unignored project files, and symlinks are never followed.
    with archive.open("wb") as output, gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w") as source:
            for name in sorted(set(files)):
                if not name:
                    continue
                path = ROOT / name
                if path.is_symlink() or not path.is_file():
                    raise RuntimeError(f"Unsupported source path: {name}")
                payload = path.read_bytes()
                info = tarfile.TarInfo(f"fNET-{version}/{name}")
                info.size = len(payload)
                info.mode = 0o644
                source.addfile(info, io.BytesIO(payload))
    crate = directory / f"fNET-{version}.crate"
    shutil.copyfile(target / "package" / crate.name, crate)
    notes_file = directory / "RELEASE-NOTES.md"
    notes_file.write_text(notes, encoding="utf-8", newline="\n")
    (directory / "SHA256SUMS").write_text("".join(f"{digest(path)}  {path.name}\n" for path in [crate, archive, notes_file]), encoding="utf-8", newline="\n")
    print(f"Prepared release artifacts in {directory}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check", "artifacts"])
    parser.add_argument("--tag")
    parser.add_argument("--require-tag", action="store_true")
    parser.add_argument("--allow-dirty", action="store_true")
    parser.add_argument("--offline", action="store_true", help="Use offline cargo package verification instead of a registry publish dry run")
    options = parser.parse_args()
    try:
        {"check": check, "artifacts": artifacts}[options.command](options)
    except (RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release validation failed: {error}\n")


if __name__ == "__main__":
    main()
