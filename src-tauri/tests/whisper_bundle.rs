#[cfg(target_os = "macos")]
mod tests {
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const UNIVERSAL_LIBS: &[&str] = &[
        "libggml.0.dylib",
        "libggml-cpu.0.dylib",
        "libggml-blas.0.dylib",
        "libggml-metal.0.dylib",
        "libggml-base.0.dylib",
        "libwhisper.1.dylib",
    ];

    #[derive(serde::Deserialize)]
    struct Manifest {
        artifacts: Vec<Artifact>,
    }

    #[derive(serde::Deserialize)]
    struct Artifact {
        target: String,
        path: String,
        sha256: String,
    }

    fn resources_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/whispercpp")
    }

    fn run_file(path: &Path) -> String {
        let output = Command::new("file")
            .arg(path)
            .output()
            .expect("`file` should run");
        assert!(
            output.status.success(),
            "`file` failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn sha256_file(path: &Path) -> String {
        let bytes = fs::read(path).expect("binary should be readable");
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    #[test]
    fn bundled_whisper_binaries_match_manifest_and_architectures() {
        let resources_dir = resources_dir();
        let manifest_path = resources_dir.join("manifest.json");
        let manifest: Manifest = serde_json::from_str(
            &fs::read_to_string(&manifest_path).expect("manifest should be readable"),
        )
        .expect("manifest should parse");

        for artifact in manifest.artifacts {
            let binary_path = resources_dir.join(
                artifact
                    .path
                    .strip_prefix("whispercpp/")
                    .expect("artifact path should be rooted in whispercpp"),
            );

            assert!(
                binary_path.is_file(),
                "missing bundled binary {}",
                binary_path.display()
            );
            assert_eq!(
                sha256_file(&binary_path),
                artifact.sha256.to_lowercase(),
                "checksum mismatch for {}",
                artifact.target
            );

            let file_detail = run_file(&binary_path).to_ascii_lowercase();
            let expected_arch = match artifact.target.as_str() {
                "macos-aarch64" => "arm64",
                "macos-x86_64" => "x86_64",
                other => panic!("unexpected target in manifest: {other}"),
            };

            assert!(
                file_detail.contains(expected_arch),
                "expected {} binary to contain {}, got: {}",
                artifact.target,
                expected_arch,
                file_detail.trim()
            );
        }
    }

    #[test]
    fn bundled_whisper_shared_libs_are_universal() {
        let lib_dir = resources_dir().join("lib");
        for lib_name in UNIVERSAL_LIBS {
            let lib_path = lib_dir.join(lib_name);
            assert!(
                lib_path.is_file(),
                "missing bundled library {}",
                lib_path.display()
            );

            let file_detail = run_file(&lib_path).to_ascii_lowercase();
            assert!(
                file_detail.contains("arm64") && file_detail.contains("x86_64"),
                "expected {} to be universal, got: {}",
                lib_path.display(),
                file_detail.trim()
            );
        }
    }
}
