import { spawn } from 'node:child_process';
import { syncVersions } from './sync-versions.mjs';

async function run(command, args) {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: 'inherit',
      shell: process.platform === 'win32',
    });

    child.on('exit', (code) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`${command} ${args.join(' ')} exited with code ${code ?? 'unknown'}`));
    });
    child.on('error', reject);
  });
}

async function main() {
  await run('pnpm', ['exec', 'changeset', 'version']);
  await syncVersions();
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
