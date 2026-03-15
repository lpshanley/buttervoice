import { readFile } from 'node:fs/promises';
import { execSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, '..');
const packageJsonPath = path.join(projectRoot, 'package.json');

async function main() {
  const packageJson = JSON.parse(await readFile(packageJsonPath, 'utf8'));
  const tag = `v${packageJson.version}`;

  const existingTags = execSync('git tag --list', { encoding: 'utf8' }).split('\n');
  if (existingTags.includes(tag)) {
    console.log(`Tag ${tag} already exists, skipping.`);
    return;
  }

  execSync(`git tag ${tag}`, { stdio: 'inherit' });
  execSync(`git push origin ${tag}`, { stdio: 'inherit' });
  console.log(`Created and pushed tag ${tag}`);
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
