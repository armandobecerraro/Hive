import { spawn } from "child_process";
import * as fs from "fs";
import * as path from "path";

export type StackOpt = "auto" | "rust_binary" | "python_app" | "node_minimal";
export type WorkModeOpt = "auto" | "build" | "improve";

export interface HiveRequestPayload {
  title: string;
  description: string;
  stack: StackOpt;
  force_scaffold: boolean;
  work_mode: WorkModeOpt;
}

export function deriveTitle(description: string): string {
  const line = description.split(/\r?\n/)[0]?.trim() ?? "";
  if (!line) {
    return "Misión Hive";
  }
  return line.length > 72 ? line.slice(0, 72) : line;
}

export function buildHiveRequest(
  description: string,
  stack: StackOpt,
  workMode: WorkModeOpt,
  forceScaffold: boolean
): HiveRequestPayload {
  return {
    title: deriveTitle(description),
    description: description.trim(),
    stack,
    force_scaffold: forceScaffold,
    work_mode: workMode,
  };
}

export function writeHiveRequestJson(
  repoRoot: string,
  payload: HiveRequestPayload
): string {
  const p = path.join(repoRoot, "hive.request.json");
  const body = JSON.stringify(payload, null, 2) + "\n";
  fs.writeFileSync(p, body, "utf8");
  return p;
}

export type LogChunk = { channel: "stdout" | "stderr"; text: string };

export function runHiveOnce(
  repoRoot: string,
  executable: string,
  onChunk: (c: LogChunk) => void
): Promise<number> {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, ["--once", repoRoot], {
      cwd: repoRoot,
      env: { ...process.env },
      shell: false,
    });

    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");

    child.stdout.on("data", (d: string) =>
      onChunk({ channel: "stdout", text: d })
    );
    child.stderr.on("data", (d: string) =>
      onChunk({ channel: "stderr", text: d })
    );

    child.on("error", (err: NodeJS.ErrnoException) => {
      reject(err);
    });

    child.on("close", (code) => {
      resolve(code ?? 1);
    });
  });
}
