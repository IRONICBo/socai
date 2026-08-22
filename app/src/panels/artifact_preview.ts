import Papa from "papaparse";

import type { AgentArtifact } from "../main";
import { esc } from "../lib/html";
import { t } from "../lib/i18n";
import { renderArtifactMarkdown } from "../lib/markdown";
import type { ArtifactDownloadState } from "./conversation";

export interface ArtifactPreviewPaneState {
  taskId: string;
  path: string;
  name: string;
  kind: string;
  sizeBytes: number;
  version: string;
  previewKind: NonNullable<AgentArtifact["preview_kind"]>;
  status: "loading" | "ready" | "error";
  text?: string;
  blobUrl?: string;
  error?: string;
}

export function renderArtifactPreview(
  preview: ArtifactPreviewPaneState | null,
  downloadState: ArtifactDownloadState | undefined,
): string {
  if (!preview) return "";
  const openingState = downloadState?.status === "downloaded"
    || downloadState?.status === "opening"
    || downloadState?.status === "open_failed";
  const downloadLabel = downloadState?.status === "downloading"
    ? t("artifact.downloadingAria", { name: preview.name })
    : downloadState?.status === "download_failed"
      ? t("artifact.downloadFailedAria", { name: preview.name })
      : downloadState?.status === "opening"
        ? t("artifact.openingAria", { name: preview.name })
        : downloadState?.status === "open_failed"
          ? t("artifact.openFailedAria", { name: preview.name })
          : openingState
            ? t("artifact.openAria", { name: preview.name })
            : t("artifact.downloadAria", { name: preview.name });
  return `
    <aside class="artifact-preview" aria-label="${esc(t("artifact.previewPanelAria", { name: preview.name }))}">
      <header class="artifact-preview__head">
        <div class="artifact-preview__identity">
          <span class="artifact-preview__name" title="${esc(preview.name)}">${esc(preview.name)}</span>
          <span class="artifact-preview__meta">${esc(preview.kind)} · ${esc(formatArtifactSize(preview.sizeBytes))}</span>
        </div>
        <div class="artifact-preview__actions">
          <button
            type="button"
            class="artifact-preview__action${downloadState?.status === "open_failed" || downloadState?.status === "download_failed" ? " is-error" : ""}"
            data-artifact-action="${esc(preview.taskId)}"
            data-artifact-path="${esc(preview.path)}"
            title="${esc(downloadLabel)}"
            aria-label="${esc(downloadLabel)}"
            ${downloadState?.status === "downloading" || downloadState?.status === "opening" ? 'aria-disabled="true" aria-busy="true"' : ""}
          >${downloadIcon()}</button>
          <button type="button" class="artifact-preview__action" data-artifact-preview-close aria-label="${esc(t("artifact.previewClose"))}" title="${esc(t("artifact.previewClose"))}">${closeIcon()}</button>
        </div>
      </header>
      <div class="artifact-preview__body">
        ${renderPreviewBody(preview)}
      </div>
    </aside>
  `;
}

function renderPreviewBody(preview: ArtifactPreviewPaneState): string {
  if (preview.status === "loading") {
    return `<div class="artifact-preview__state"><span class="spinner" aria-hidden="true"></span>${esc(t("artifact.previewLoading"))}</div>`;
  }
  if (preview.status === "error") {
    return `<div class="artifact-preview__state artifact-preview__state--error"><span>${esc(t("artifact.previewFailed"))}</span><small>${esc(preview.error ?? "")}</small></div>`;
  }
  if (preview.previewKind === "markdown") {
    return `<article class="artifact-preview__markdown result-md">${renderArtifactMarkdown(preview.text ?? "")}</article>`;
  }
  if (preview.previewKind === "csv") {
    return renderCsv(preview.text ?? "", preview.kind === "TSV" ? "\t" : ",");
  }
  if (preview.previewKind === "text") {
    return `<pre class="artifact-preview__text">${esc(formatTextPreview(preview.name, preview.text ?? ""))}</pre>`;
  }
  if (preview.previewKind === "pdf" && preview.blobUrl) {
    return `<object class="artifact-preview__pdf" data="${esc(preview.blobUrl)}" type="application/pdf"><p>${esc(t("artifact.previewPdfUnavailable"))}</p></object>`;
  }
  if (preview.previewKind === "image" && preview.blobUrl) {
    return `<div class="artifact-preview__image-wrap"><img class="artifact-preview__image" src="${esc(preview.blobUrl)}" alt="${esc(preview.name)}" /></div>`;
  }
  return `<div class="artifact-preview__state artifact-preview__state--error">${esc(t("artifact.previewFailed"))}</div>`;
}

export function artifactPreviewMime(name: string, kind: ArtifactPreviewPaneState["previewKind"]): string {
  if (kind === "pdf") return "application/pdf";
  if (kind !== "image") return "text/plain;charset=utf-8";
  const extension = name.split(".").pop()?.toLowerCase();
  if (extension === "png") return "image/png";
  if (extension === "jpg" || extension === "jpeg") return "image/jpeg";
  if (extension === "gif") return "image/gif";
  if (extension === "webp") return "image/webp";
  if (extension === "bmp") return "image/bmp";
  return "application/octet-stream";
}

function renderCsv(text: string, delimiter: string): string {
  const maxRows = 500;
  const maxColumns = 80;
  const maxParseCharacters = 128 * 1024;
  const maxCellCharacters = 8 * 1024;
  const parseText = text.slice(0, maxParseCharacters);
  const parsed = Papa.parse<string[]>(parseText, {
    delimiter,
    skipEmptyLines: false,
    // Stop before newline-heavy files allocate an array for every row. The
    // character cap also bounds work for one delimiter-heavy row.
    preview: maxRows + 1,
  });
  let truncated = text.length > maxParseCharacters || parsed.data.length > maxRows;
  const rows = parsed.data.slice(0, maxRows).map((row) => {
    if (row.length > maxColumns) truncated = true;
    return row.slice(0, maxColumns).map((cell) => {
      if (cell.length <= maxCellCharacters) return cell;
      truncated = true;
      return `${cell.slice(0, maxCellCharacters)}…`;
    });
  });
  const columnCount = Math.min(
    maxColumns,
    rows.reduce((maximum, row) => Math.max(maximum, row.length), 0),
  );
  const body = rows.map((row, rowIndex) => `
    <tr>
      <th class="artifact-preview__row-number" scope="row">${rowIndex + 1}</th>
      ${Array.from({ length: columnCount }, (_, columnIndex) => {
        const tag = rowIndex === 0 ? "th" : "td";
        const scope = rowIndex === 0 ? " scope=\"col\"" : "";
        return `<${tag}${scope}>${esc(row[columnIndex] ?? "")}</${tag}>`;
      }).join("")}
    </tr>
  `).join("");
  return `
    <div class="artifact-preview__sheet">
      <table><tbody>${body}</tbody></table>
      ${truncated ? `<p class="artifact-preview__limit">${esc(t("artifact.previewTableLimit", { rows: maxRows, columns: maxColumns }))}</p>` : ""}
    </div>
  `;
}

function formatTextPreview(name: string, text: string): string {
  if (!/\.json$/i.test(name)) return text;
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

export function formatArtifactSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(bytes < 10 * 1024 * 1024 ? 1 : 0)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function artifactFileIcon(): string {
  return `<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6.5 3.5h7l4 4v13h-11z"></path><path d="M13.5 3.5v4h4"></path><path d="M9 13h6M9 16h4"></path></svg>`;
}

export function eyeIcon(): string {
  return `<svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 12s3.4-5.5 9.5-5.5 9.5 5.5 9.5 5.5-3.4 5.5-9.5 5.5S2.5 12 2.5 12z"></path><circle cx="12" cy="12" r="2.4"></circle></svg>`;
}

export function downloadIcon(): string {
  return `<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v11"></path><path d="m8 10 4 4 4-4"></path><path d="M5 17v3h14v-3"></path></svg>`;
}

function closeIcon(): string {
  return `<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><path d="m6 6 12 12M18 6 6 18"></path></svg>`;
}
