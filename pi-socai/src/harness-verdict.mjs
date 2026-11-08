export function durableHarnessVerdict(result) {
  const issues = [];
  const runStatus = result.run.ok ? result.run.value.status : "admission_failed";

  if (!result.run.ok) issues.push("Pi did not admit the run");
  else if (runStatus !== "completed") issues.push(`Pi run ended as ${runStatus}`);

  if (!result.toolEnd) {
    issues.push("Pi emitted no tool_end event");
  } else if (result.toolEnd.isError) {
    const message = result.toolEnd.result?.content
      ?.filter((item) => item.type === "text")
      .map((item) => item.text)
      .join(" ")
      .trim();
    issues.push(`socai tool failed${message ? `: ${message}` : ""}`);
  } else if (result.toolEnd.result?.details?.status !== "completed") {
    issues.push("socai tool did not return a completed result");
  }

  if (result.snapshot.faulted) issues.push("Pi session is faulted");
  if (result.snapshot.operation !== null) issues.push("Pi operation remained open");

  return { ok: issues.length === 0, runStatus, issues };
}
