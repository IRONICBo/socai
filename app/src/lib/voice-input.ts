import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";

export type WhisperModelSize = "low" | "medium" | "high";
export type VoicePhase = "idle" | "requesting" | "recording" | "transcribing";

export interface WhisperStatus {
  ready: boolean;
  state: "ready" | "binary_missing" | "model_missing" | "downloading" | "error";
  binary_available: boolean;
  binary_path: string | null;
  model_available: boolean;
  model_size: WhisperModelSize;
  model_name: string;
  model_path: string;
  model_bytes: number;
  downloaded_bytes: number;
  total_bytes: number;
  error: string | null;
}

export interface ComposerVoiceState {
  available: boolean;
  phase: VoicePhase;
  title: string;
  error: string;
}

interface WhisperProgress {
  model_size: WhisperModelSize;
  downloaded_bytes: number;
  total_bytes: number;
}

interface WhisperTranscript {
  text: string;
}

const TARGET_SAMPLE_RATE = 16_000;
const MAX_RECORDING_MS = 120_000;

export namespace voiceInput {
  let status: WhisperStatus | null = null;
  let phase: VoicePhase = "idle";
  let operationError = "";
  let onChange: () => void = () => {};
  let initialized = false;
  let lastRenderedPercent = -1;

  let stream: MediaStream | null = null;
  let audioContext: AudioContext | null = null;
  let sourceNode: MediaStreamAudioSourceNode | null = null;
  let processorNode: ScriptProcessorNode | null = null;
  let silentGain: GainNode | null = null;
  let chunks: Float32Array[] = [];
  let capturedSampleRate = TARGET_SAMPLE_RATE;
  let recordingTimer: number | null = null;
  let captureGeneration = 0;

  export async function initialize(changeHandler: () => void): Promise<void> {
    onChange = changeHandler;
    if (!initialized) {
      initialized = true;
      await listen<WhisperProgress>("whisper:model-progress", (event) => {
        const progress = event.payload;
        if (!status || progress.model_size !== status.model_size) return;
        status = {
          ...status,
          ready: false,
          state: "downloading",
          downloaded_bytes: progress.downloaded_bytes,
          total_bytes: progress.total_bytes,
        };
        const percent = downloadPercent();
        if (percent !== lastRenderedPercent) {
          lastRenderedPercent = percent;
          onChange();
        }
      });
    }
    await refreshStatus();
  }

  export function getStatus(): WhisperStatus | null {
    return status;
  }

  export function getPhase(): VoicePhase {
    return phase;
  }

  export function isBusy(): boolean {
    return phase !== "idle";
  }

  export function downloadPercent(): number {
    if (!status?.total_bytes) return 0;
    return Math.min(100, Math.max(0, Math.round((status.downloaded_bytes / status.total_bytes) * 100)));
  }

  export function modelSizeLabel(size: WhisperModelSize): string {
    return t(`voice.model.${size}` as "voice.model.low" | "voice.model.medium" | "voice.model.high");
  }

  export function statusText(): string {
    if (!status) return t("common.loading");
    switch (status.state) {
      case "ready":
        return t("voice.status.ready", { model: modelSizeLabel(status.model_size) });
      case "binary_missing":
        return t("voice.status.binaryMissing");
      case "model_missing":
        return t("voice.status.modelMissing");
      case "downloading":
        return t("voice.status.downloading", { percent: downloadPercent() });
      case "error":
        return t("voice.status.failed");
    }
  }

  export function composerState(): ComposerVoiceState {
    const mediaSupported = !!navigator.mediaDevices?.getUserMedia && typeof AudioContext !== "undefined";
    const available = !!status?.ready && mediaSupported;
    let title: string;
    if (phase === "recording") title = t("voice.stop");
    else if (phase === "requesting") title = t("voice.requesting");
    else if (phase === "transcribing") title = t("voice.transcribing");
    else if (!mediaSupported) title = t("voice.unavailable.browser");
    else if (!status) title = t("voice.unavailable.checking");
    else if (status.state === "binary_missing") title = t("voice.unavailable.installFailed");
    else if (status.state === "downloading") {
      title = t("voice.unavailable.downloading", { percent: downloadPercent() });
    } else if (status.state === "model_missing") title = t("voice.unavailable.modelMissing");
    else if (status.state === "error") title = t("voice.unavailable.failed");
    else title = t("voice.start");
    return { available, phase, title, error: operationError };
  }

  export async function selectModel(size: WhisperModelSize): Promise<void> {
    if (phase !== "idle" || status?.model_size === size && status.ready) return;
    operationError = "";
    if (status) {
      status = {
        ...status,
        ready: false,
        state: "downloading",
        model_size: size,
        downloaded_bytes: 0,
      };
    }
    lastRenderedPercent = -1;
    onChange();
    try {
      status = await invoke<WhisperStatus>("whisper_select_model", { size });
    } catch (error) {
      console.error("whisper_select_model failed:", error);
      operationError = `${error}`;
      await refreshStatus();
    } finally {
      onChange();
    }
  }

  /** Start on the first click; stop, transcribe, and return text on the next. */
  export async function toggle(): Promise<string | null> {
    if (phase === "recording") return stopAndTranscribe();
    if (phase !== "idle" || !composerState().available) return null;
    await startRecording();
    return null;
  }

  export function cancelRecording(message = ""): void {
    if (phase !== "recording" && phase !== "requesting") return;
    captureGeneration += 1;
    cleanupRecording();
    phase = "idle";
    operationError = message;
    onChange();
  }

  async function refreshStatus(): Promise<void> {
    try {
      status = await invoke<WhisperStatus>("whisper_status");
    } catch (error) {
      console.error("whisper_status failed:", error);
      status = null;
      operationError = `${error}`;
    }
  }

  async function startRecording(): Promise<void> {
    const generation = captureGeneration + 1;
    captureGeneration = generation;
    phase = "requesting";
    operationError = "";
    onChange();
    try {
      const capturedStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
        video: false,
      });
      if (generation !== captureGeneration) {
        capturedStream.getTracks().forEach((track) => track.stop());
        return;
      }
      stream = capturedStream;
      audioContext = new AudioContext({ sampleRate: TARGET_SAMPLE_RATE });
      capturedSampleRate = audioContext.sampleRate;
      chunks = [];
      sourceNode = audioContext.createMediaStreamSource(stream);
      processorNode = audioContext.createScriptProcessor(4096, 1, 1);
      silentGain = audioContext.createGain();
      silentGain.gain.value = 0;
      processorNode.onaudioprocess = (event) => {
        chunks.push(new Float32Array(event.inputBuffer.getChannelData(0)));
        event.outputBuffer.getChannelData(0).fill(0);
      };
      sourceNode.connect(processorNode);
      processorNode.connect(silentGain);
      silentGain.connect(audioContext.destination);
      phase = "recording";
      recordingTimer = window.setTimeout(() => {
        cancelRecording(t("voice.error.maxDuration"));
      }, MAX_RECORDING_MS);
    } catch (error) {
      console.error("microphone capture failed:", error);
      cleanupRecording();
      phase = "idle";
      operationError = microphoneError(error);
    }
    onChange();
  }

  async function stopAndTranscribe(): Promise<string | null> {
    captureGeneration += 1;
    phase = "transcribing";
    operationError = "";
    const samples = mergeChunks(chunks);
    const sourceRate = capturedSampleRate;
    cleanupRecording();
    onChange();

    try {
      if (samples.length < sourceRate / 4) {
        throw new Error(t("voice.error.tooShort"));
      }
      const mono16k = resampleMono(samples, sourceRate, TARGET_SAMPLE_RATE);
      const wav = encodePcm16Wav(mono16k, TARGET_SAMPLE_RATE);
      const audioBase64 = bytesToBase64(wav);
      const result = await invoke<WhisperTranscript>("whisper_transcribe", { audioBase64 });
      const text = result.text.trim();
      if (!text) throw new Error(t("voice.error.noSpeech"));
      return text;
    } catch (error) {
      console.error("whisper_transcribe failed:", error);
      operationError = error instanceof Error ? error.message : `${error}`;
      return null;
    } finally {
      phase = "idle";
      onChange();
    }
  }

  function cleanupRecording(): void {
    if (recordingTimer !== null) window.clearTimeout(recordingTimer);
    recordingTimer = null;
    if (processorNode) {
      processorNode.onaudioprocess = null;
      processorNode.disconnect();
    }
    sourceNode?.disconnect();
    silentGain?.disconnect();
    stream?.getTracks().forEach((track) => track.stop());
    if (audioContext && audioContext.state !== "closed") void audioContext.close();
    stream = null;
    audioContext = null;
    sourceNode = null;
    processorNode = null;
    silentGain = null;
    chunks = [];
  }

  function microphoneError(error: unknown): string {
    const name = error instanceof DOMException ? error.name : "";
    if (name === "NotAllowedError" || name === "SecurityError") return t("voice.error.permission");
    if (name === "NotFoundError" || name === "DevicesNotFoundError") return t("voice.error.noDevice");
    return t("voice.error.capture");
  }
}

function mergeChunks(chunks: Float32Array[]): Float32Array {
  const length = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const merged = new Float32Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.length;
  }
  return merged;
}

function resampleMono(input: Float32Array, sourceRate: number, targetRate: number): Float32Array {
  if (sourceRate === targetRate) return input;
  const ratio = sourceRate / targetRate;
  const output = new Float32Array(Math.max(1, Math.floor(input.length / ratio)));
  for (let index = 0; index < output.length; index += 1) {
    const start = Math.floor(index * ratio);
    const end = Math.min(input.length, Math.max(start + 1, Math.floor((index + 1) * ratio)));
    let sum = 0;
    for (let cursor = start; cursor < end; cursor += 1) sum += input[cursor];
    output[index] = sum / (end - start);
  }
  return output;
}

function encodePcm16Wav(samples: Float32Array, sampleRate: number): Uint8Array {
  const dataBytes = samples.length * 2;
  const buffer = new ArrayBuffer(44 + dataBytes);
  const view = new DataView(buffer);
  writeAscii(view, 0, "RIFF");
  view.setUint32(4, 36 + dataBytes, true);
  writeAscii(view, 8, "WAVE");
  writeAscii(view, 12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeAscii(view, 36, "data");
  view.setUint32(40, dataBytes, true);
  for (let index = 0; index < samples.length; index += 1) {
    const value = Math.max(-1, Math.min(1, samples[index]));
    view.setInt16(44 + index * 2, value < 0 ? value * 0x8000 : value * 0x7fff, true);
  }
  return new Uint8Array(buffer);
}

function writeAscii(view: DataView, offset: number, value: string): void {
  for (let index = 0; index < value.length; index += 1) {
    view.setUint8(offset + index, value.charCodeAt(index));
  }
}

function bytesToBase64(bytes: Uint8Array): string {
  const blockSize = 0x8000;
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += blockSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + blockSize));
  }
  return btoa(binary);
}
