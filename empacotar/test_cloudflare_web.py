"""Orquestração do Pages, sem rede, rustup real ou compilação."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("cloudflare-web.sh")


class BuildPages(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="seele-pages-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "empacotar").mkdir()
        shutil.copyfile(SCRIPT, self.root / "empacotar/cloudflare-web.sh")
        (self.root / "Cargo.lock").write_text("lock fixo\n")
        (self.root / "rust-toolchain.toml").write_text('channel = "1.97.1"\n')
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.env = dict(os.environ, PATH=f"{self.bin}:/usr/bin:/bin")
        self.env.pop("CARGO_TARGET_DIR", None)
        self.tool("uname", 'case "$1" in -s) echo Linux;; -m) echo x86_64;; esac')
        for nome in ("rustup", "rustc", "cc"):
            self.tool(nome, 'echo "' + nome + ' $*" >> ferramentas.log')
        self.tool("curl", 'echo "download inesperado" >&2; exit 70')
        # SHA-256 real, inclusive no Mac que não traz sha256sum no PATH.
        self.tool("sha256sum", 'exec /usr/bin/shasum -a 256 "$@"' if not Path('/usr/bin/sha256sum').exists() else 'exec /usr/bin/sha256sum "$@"')
        self.tool("cargo", '''echo "cargo $*" >> ferramentas.log
case "$*" in
  *pacote-web)
    [ "${FALHAR_PACOTE:-0}" = 0 ] || exit 42
    mkdir -p target/pacote-web
    for f in index.html seele_web.wasm seele_motor_web.wasm _headers; do echo teste > "target/pacote-web/$f"; done
    [ "${MUDAR_LOCK:-0}" = 0 ] || echo alterado >> Cargo.lock
    ;;
esac''')

    def tool(self, nome, corpo):
        p = self.bin / nome
        p.write_text("#!/bin/sh\nset -eu\n" + corpo + "\n")
        p.chmod(0o755)

    def run_build(self):
        return subprocess.run(["/bin/sh", "empacotar/cloudflare-web.sh"], cwd=self.root,
                              env=self.env, text=True, capture_output=True)

    def test_ordem_locked_e_saida(self):
        r = self.run_build()
        self.assertEqual(r.returncode, 0, r.stderr)
        log = (self.root / "ferramentas.log").read_text()
        self.assertLess(log.index("-- wasi-sdk"), log.index("-- pacote-web"))
        self.assertEqual(log.count("cargo run --quiet --locked --package xtask"), 2)
        self.assertIn("--toolchain 1.97.1 wasm32-unknown-unknown wasm32-wasip1", log)
        self.assertFalse((self.root / "target/.cloudflare-web.lock").exists())

    def test_falha_de_build_propaga_e_libera_trava(self):
        self.env["FALHAR_PACOTE"] = "1"
        self.assertEqual(self.run_build().returncode, 42)
        self.assertFalse((self.root / "target/.cloudflare-web.lock").exists())

    def test_lockfile_alterado_recusa(self):
        self.env["MUDAR_LOCK"] = "1"
        r = self.run_build()
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("Cargo.lock mudou", r.stderr)

    def test_build_concorrente_nao_remove_trava_do_outro(self):
        trava = self.root / "target/.cloudflare-web.lock"
        trava.mkdir(parents=True)
        r = self.run_build()
        self.assertNotEqual(r.returncode, 0)
        self.assertTrue(trava.exists())
        self.assertFalse((self.root / "ferramentas.log").exists())

    def test_target_externo_recusa(self):
        self.env["CARGO_TARGET_DIR"] = "/tmp/outro-target"
        self.assertIn("CARGO_TARGET_DIR", self.run_build().stderr)
        self.assertFalse((self.root / "ferramentas.log").exists())

    def test_bootstrap_corrompido_nao_executa(self):
        (self.bin / "rustup").unlink()
        self.tool("curl", '''while [ "$1" != -o ]; do shift; done
shift
printf '#!/bin/sh\ntouch executou-instalador\n' > "$1"''')
        r = self.run_build()
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse((self.root / "executou-instalador").exists())
        self.assertFalse((self.root / "target/.cloudflare-web.lock").exists())


if __name__ == "__main__":
    unittest.main()
