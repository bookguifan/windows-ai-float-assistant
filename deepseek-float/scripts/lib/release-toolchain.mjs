import path from 'node:path';

function parseStableVersion(value, label) {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(value);
  if (!match) throw new Error(`${label} must be a stable x.y.z version; received ${value}`);
  return match.slice(1).map(Number);
}

function compareVersions(left, right) {
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

export function assertNodeEngineVersion(version, engineRange) {
  const range = /^\^(\d+)\.(\d+)\.(\d+)$/.exec(engineRange);
  if (!range) throw new Error(`Unsupported Node engine range: ${engineRange}`);

  const floor = range.slice(1).map(Number);
  const actual = parseStableVersion(version, 'Node version');
  const upperBoundMatches = floor[0] > 0
    ? actual[0] === floor[0]
    : floor[1] > 0
      ? actual[0] === 0 && actual[1] === floor[1]
      : actual[0] === 0 && actual[1] === 0;

  if (!upperBoundMatches || compareVersions(actual, floor) < 0) {
    throw new Error(`Release requires Node ${engineRange}; current Node is ${version}`);
  }
  return true;
}

export function pinnedPnpmVersion(packageManager) {
  const match = /^pnpm@(\d+\.\d+\.\d+)$/.exec(packageManager);
  if (!match) throw new Error(`packageManager must pin pnpm exactly; received ${packageManager}`);
  return match[1];
}

export function nodeBoundEnvironment(environment, nodeExecutable, platform = process.platform) {
  const pathKey = Object.keys(environment).find((key) => key.toLowerCase() === 'path') || 'PATH';
  const pathModule = platform === 'win32' ? path.win32 : path;
  const nodeDirectory = pathModule.dirname(nodeExecutable);
  const delimiter = platform === 'win32' ? ';' : path.delimiter;
  const existingPath = environment[pathKey];

  return {
    ...environment,
    [pathKey]: [nodeDirectory, existingPath].filter(Boolean).join(delimiter),
  };
}

export function assertPnpmRuntime({
  actualPnpmVersion,
  expectedPnpmVersion,
  actualNodeExecutable,
  expectedNodeExecutable,
  actualNodeVersion,
  expectedNodeVersion,
  platform = process.platform,
}) {
  if (actualPnpmVersion !== expectedPnpmVersion) {
    throw new Error(`Release requires pnpm ${expectedPnpmVersion}; found ${actualPnpmVersion}`);
  }

  const pathModule = platform === 'win32' ? path.win32 : path;
  const normalizeExecutable = (value) => {
    const normalized = pathModule.normalize(value);
    return platform === 'win32' ? normalized.toLowerCase() : normalized;
  };
  if (normalizeExecutable(actualNodeExecutable) !== normalizeExecutable(expectedNodeExecutable)) {
    throw new Error(`pnpm exec resolved a different Node executable: ${actualNodeExecutable}`);
  }
  if (actualNodeVersion !== expectedNodeVersion) {
    throw new Error(`pnpm exec resolved a different Node version: ${actualNodeVersion}`);
  }
  return true;
}
