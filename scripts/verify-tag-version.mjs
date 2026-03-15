import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, '..');
const packageJsonPath = path.join(projectRoot, 'package.json');

async function main() {
  const packageJson = JSON.parse(await readFile(packageJsonPath, 'utf8'));
  const expectedTag = `v${packageJson.version}`;
  const actualTag = process.env.GITHUB_REF_NAME || process.argv[2];

  if (!actualTag) {
    throw new Error('No tag name provided. Set GITHUB_REF_NAME or pass the tag as an argument.');
  }

  if (actualTag !== expectedTag) {
    throw new Error(
      `Release tag ${actualTag} does not match package version ${packageJson.version}. Expected ${expectedTag}.`,
    );
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
