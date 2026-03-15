import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, '..');
const packageJsonPath = path.join(projectRoot, 'package.json');
const cargoTomlPath = path.join(projectRoot, 'src-tauri', 'Cargo.toml');
const tauriConfigPath = path.join(projectRoot, 'src-tauri', 'tauri.conf.json');

function updateCargoVersion(contents, version) {
  return contents.replace(
    /(\[package\][\s\S]*?\nversion\s*=\s*")([^"]+)(")/,
    `$1${version}$3`,
  );
}

function updateTauriConfigVersion(contents, version) {
  const config = JSON.parse(contents);
  config.version = version;
  return `${JSON.stringify(config, null, 2)}\n`;
}

async function loadPackageVersion() {
  const packageJson = JSON.parse(await readFile(packageJsonPath, 'utf8'));
  return packageJson.version;
}

export async function syncVersions({ check = false } = {}) {
  const version = await loadPackageVersion();
  const cargoToml = await readFile(cargoTomlPath, 'utf8');
  const tauriConfig = await readFile(tauriConfigPath, 'utf8');

  const nextCargoToml = updateCargoVersion(cargoToml, version);
  const nextTauriConfig = updateTauriConfigVersion(tauriConfig, version);

  const driftedFiles = [];
  if (nextCargoToml !== cargoToml) driftedFiles.push('src-tauri/Cargo.toml');
  if (nextTauriConfig !== tauriConfig) driftedFiles.push('src-tauri/tauri.conf.json');

  if (check) {
    if (driftedFiles.length > 0) {
      throw new Error(
        `Version drift detected. Run "pnpm release:version" or "node ./scripts/sync-versions.mjs". Out of sync: ${driftedFiles.join(', ')}`,
      );
    }
    return version;
  }

  if (driftedFiles.length > 0) {
    await writeFile(cargoTomlPath, nextCargoToml);
    await writeFile(tauriConfigPath, nextTauriConfig);
  }

  return version;
}

async function main() {
  const args = new Set(process.argv.slice(2));

  if (args.has('--print-version')) {
    process.stdout.write(`${await loadPackageVersion()}\n`);
    return;
  }

  await syncVersions({ check: args.has('--check') });
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
