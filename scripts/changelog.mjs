/** Deterministic Changesets renderer. Release prose is reviewed in change fragments. */
export function getReleaseLine(changeset, type) {
  if (!['major', 'minor', 'patch'].includes(type)) throw new Error('Unknown release type');
  if (typeof changeset.summary !== 'string' || !changeset.summary.trim()) {
    throw new Error('A release note must describe the user-visible change');
  }
  const lines = changeset.summary.trim().split(/\r?\n/u);
  return lines.map((line, index) => line ? `${index === 0 ? '- ' : '  '}${line}` : '').join('\n');
}

export function getDependencyReleaseLine(_changesets, dependencies) {
  if (!Array.isArray(dependencies)) throw new Error('Expected dependency releases');
  const entries = dependencies.map(({ name, newVersion }) => {
    if (typeof name !== 'string' || !name || typeof newVersion !== 'string' || !newVersion) {
      throw new Error('Dependency releases require a name and version');
    }
    return `${name}@${newVersion}`;
  });
  return [...new Set(entries)].sort().map(entry => `- Updated dependency: ${entry}`).join('\n');
}

export default { getReleaseLine, getDependencyReleaseLine };
