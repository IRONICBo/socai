function visit(value, state, path = []) {
  if (Array.isArray(value)) {
    const key = path.at(-1) ?? "items";
    state.arrayCounts[key] = Math.max(state.arrayCounts[key] ?? 0, value.length);
    for (const item of value.slice(0, 20)) visit(item, state, path);
    return;
  }
  if (!value || typeof value !== "object") return;
  for (const [key, child] of Object.entries(value)) {
    if (state.runDir === undefined && key === "run_dir" && typeof child === "string") {
      state.runDir = child;
    }
    visit(child, state, [...path, key]);
  }
}

export function summarizeSocaiResult(result) {
  const state = { arrayCounts: {}, runDir: undefined };
  visit(result, state);
  return {
    topLevelKeys: result && typeof result === "object" && !Array.isArray(result)
      ? Object.keys(result).sort()
      : [],
    arrayCounts: state.arrayCounts,
    ...(state.runDir ? { runDir: state.runDir } : {}),
  };
}
