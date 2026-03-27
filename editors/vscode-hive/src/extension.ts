import * as vscode from "vscode";
import * as path from "path";
import {
  buildHiveRequest,
  runHiveOnce,
  writeHiveRequestJson,
  type LogChunk,
} from "./hiveRunner";

const PANEL_VIEW_TYPE = "hive.missionPanel";

let missionPanel: vscode.WebviewPanel | undefined;

export function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("hive.openChat", () =>
      openMissionPanel(context)
    ),
    vscode.commands.registerCommand("hive.runCycle", () => runCycleFromCommand())
  );
}

export function deactivate() {
  missionPanel = undefined;
}

function getWorkspaceRoot(): string | undefined {
  return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

function resolveExecutable(config: vscode.WorkspaceConfiguration): string {
  const custom = config.get<string>("executablePath")?.trim();
  if (custom) {
    if (!path.isAbsolute(custom)) {
      throw new Error("hive.executablePath debe ser una ruta absoluta");
    }
    if (!fs.existsSync(custom)) {
      throw new Error(`El ejecutable no existe: ${custom}`);
    }
    return custom;
  }
  return "hive_core";
}

function sanitizeRepoRoot(root: string): string {
  const resolved = path.resolve(root);
  if (!fs.existsSync(resolved)) {
    throw new Error(`El directorio no existe: ${resolved}`);
  }
  const stat = fs.statSync(resolved);
  if (!stat.isDirectory()) {
    throw new Error(`La ruta no es un directorio: ${resolved}`);
  }
  return resolved;
}

async function openMissionPanel(context: vscode.ExtensionContext) {
  const root = getWorkspaceRoot();
  if (!root) {
    void vscode.window.showWarningMessage(
      "Hive: abre una carpeta en el workspace primero."
    );
    return;
  }

  if (missionPanel) {
    missionPanel.reveal(vscode.ViewColumn.Beside);
    return;
  }

  missionPanel = vscode.window.createWebviewPanel(
    PANEL_VIEW_TYPE,
    "Hive — misión",
    vscode.ViewColumn.Beside,
    {
      enableScripts: true,
      retainContextWhenHidden: true,
      localResourceRoots: [
        vscode.Uri.joinPath(context.extensionUri, "media"),
      ],
    }
  );

  missionPanel.webview.html = getWebviewHtml(
    context.extensionUri,
    missionPanel.webview
  );

  missionPanel.webview.onDidReceiveMessage(
    async (msg: { type: string; payload?: unknown }) => {
      if (msg.type !== "run" || !msg.payload || typeof msg.payload !== "object") {
        return;
      }
      const p = msg.payload as {
        description: string;
        stack: string;
        work_mode: string;
        force_scaffold: boolean;
      };
      await executeMission(root, p);
    },
    undefined,
    context.subscriptions
  );

  missionPanel.onDidDispose(
    () => {
      missionPanel = undefined;
    },
    null,
    context.subscriptions
  );
}

async function executeMission(
  root: string,
  payload: {
    description: string;
    stack: string;
    work_mode: string;
    force_scaffold: boolean;
  }
) {
  const panel = missionPanel;
  if (!panel) {
    return;
  }

  const desc = (payload.description ?? "").trim();
  if (!desc) {
    postToWebview(panel, {
      type: "error",
      payload: { message: "Escribe una descripción de la misión." },
    });
    return;
  }

  postToWebview(panel, { type: "runStart", payload: {} });

  const stack = normalizeStack(payload.stack);
  const workMode = normalizeWorkMode(payload.work_mode);

  let safeRoot: string;
  try {
    safeRoot = sanitizeRepoRoot(root);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    postToWebview(panel, { type: "error", payload: { message: msg } });
    return;
  }

  const req = buildHiveRequest(desc, stack, workMode, !!payload.force_scaffold);
  try {
    const written = writeHiveRequestJson(root, req);
    postToWebview(panel, {
      type: "log",
      payload: {
        channel: "stdout",
        text: `[Hive] Solicitud guardada en ${written}\n`,
      },
    });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    postToWebview(panel, { type: "error", payload: { message: msg } });
    postToWebview(panel, { type: "runEnd", payload: {} });
    return;
  }

const config = vscode.workspace.getConfiguration("hive");
  let exe: string;
  try {
    exe = resolveExecutable(config);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    postToWebview(panel, { type: "error", payload: { message: msg } });
    postToWebview(panel, { type: "runEnd", payload: {} });
    return;
  }
  try {
    const written = writeHiveRequestJson(safeRoot, req);
    postToWebview(panel, {
      type: "log",
      payload: {
        channel: "stdout",
        text: `[Hive] Solicitud guardada en ${written}\n`,
      },
    });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    postToWebview(panel, { type: "error", payload: { message: msg } });
    postToWebview(panel, { type: "runEnd", payload: {} });
    return;
  }

  try {
    await runHiveProcess(panel, safeRoot, exe);
  } finally {
    postToWebview(panel, { type: "runEnd", payload: {} });
  }
}

function normalizeStack(
  s: string
): "auto" | "rust_binary" | "python_app" | "node_minimal" {
  switch (s) {
    case "rust_binary":
    case "python_app":
    case "node_minimal":
    case "auto":
      return s;
    default:
      return "auto";
  }
}

function normalizeWorkMode(s: string): "auto" | "build" | "improve" {
  switch (s) {
    case "build":
    case "improve":
    case "auto":
      return s;
    default:
      return "auto";
  }
}

async function runCycleFromCommand() {
  const root = getWorkspaceRoot();
  if (!root) {
    void vscode.window.showWarningMessage(
      "Hive: abre una carpeta en el workspace primero."
    );
    return;
  }

  let safeRoot: string;
  try {
    safeRoot = sanitizeRepoRoot(root);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    void vscode.window.showErrorMessage(`Seguridad: ${msg}`);
    return;
  }

  const config = vscode.workspace.getConfiguration("hive");
  let exe: string;
  try {
    exe = resolveExecutable(config);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    void vscode.window.showErrorMessage(`Seguridad: ${msg}`);
    return;
  }

  const reqPath = path.join(safeRoot, "hive.request.json");
  try {
    await vscode.workspace.fs.stat(vscode.Uri.file(reqPath));
  } catch {
    void vscode.window.showInformationMessage(
      "No existe hive.request.json. Usa «Hive: Abrir panel de misión» para crear la solicitud."
    );
    return;
  }

  const channel = vscode.window.createOutputChannel("Hive");
  channel.show(true);
  channel.appendLine(`> ${exe} --once ${safeRoot}`);

  try {
    const code = await runHiveOnce(safeRoot, exe, (c: LogChunk) => {
      channel.append(c.text);
    });
    channel.appendLine(`\n[código de salida: ${code}]`);
    if (code === 0) {
      void vscode.window.showInformationMessage(
        "Hive: ciclo completado correctamente."
      );
    } else {
      void vscode.window.showErrorMessage(
        `Hive: el proceso terminó con código ${code}. Ver salida «Hive».`
      );
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    channel.appendLine(`\nError: ${msg}`);
    void vscode.window.showErrorMessage(
      `No se pudo ejecutar «${exe}». Configura hive.executablePath (p. ej. ruta al binario compilado). Detalle: ${msg}`
    );
  }
}

async function runHiveProcess(
  panel: vscode.WebviewPanel,
  root: string,
  exe: string
) {
  postToWebview(panel, {
    type: "log",
    payload: { channel: "stdout", text: `\n> ${exe} --once\n\n` },
  });

  const config = vscode.workspace.getConfiguration("hive");
  if (config.get<boolean>("showTerminalOnRun")) {
    const term =
      vscode.window.terminals.find((t) => t.name === "Hive") ??
      vscode.window.createTerminal({ name: "Hive" });
    term.show();
    term.sendText(
      `${quoteExe(exe)} --once ${quotePath(root)}`,
      true
    );
  }

  try {
    const code = await runHiveOnce(root, exe, (c: LogChunk) => {
      postToWebview(panel, { type: "log", payload: c });
    });
    postToWebview(panel, { type: "done", payload: { code } });
    if (code === 0) {
      void vscode.window.showInformationMessage("Hive: ciclo completado.");
    } else {
      void vscode.window.showWarningMessage(
        `Hive terminó con código ${code}. Revisa el panel.`
      );
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    postToWebview(panel, { type: "error", payload: { message: msg } });
    void vscode.window.showErrorMessage(
      `No se pudo ejecutar «${exe}». ¿Configuraste hive.executablePath? ${msg}`
    );
  }
}

function quoteExe(s: string): string {
  if (s.includes(" ")) {
    return `"${s.replace(/"/g, '\\"')}"`;
  }
  return s;
}

function quotePath(s: string): string {
  if (process.platform === "win32") {
    return `"${s.replace(/"/g, '\\"')}"`;
  }
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

function postToWebview(
  panel: vscode.WebviewPanel,
  message: { type: string; payload?: unknown }
) {
  void panel.webview.postMessage(message);
}

function getWebviewHtml(
  extensionUri: vscode.Uri,
  webview: vscode.Webview
): string {
  const styleUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, "media", "chat.css")
  );
  const scriptUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, "media", "chat.js")
  );

  const csp = [
    "default-src 'none'",
    `style-src ${webview.cspSource}`,
    `script-src ${webview.cspSource}`,
  ].join("; ");

  return `<!DOCTYPE html>
<html lang="es">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta http-equiv="Content-Security-Policy" content="${csp}" />
  <link href="${styleUri}" rel="stylesheet" />
  <title>Hive</title>
</head>
<body>
  <header class="hive-header">
    <h1>La Colmena</h1>
    <p class="hive-sub">Describe la misión; se guarda <code>hive.request.json</code> y se ejecuta <code>hive_core --once</code>.</p>
  </header>
  <label for="desc">Misión</label>
  <textarea id="desc" rows="8" placeholder="Ej.: Añade tests al módulo X y actualiza el README."></textarea>
  <div class="hive-row">
    <div>
      <label for="stack">Stack</label>
      <select id="stack">
        <option value="auto" selected>auto</option>
        <option value="rust_binary">rust_binary</option>
        <option value="python_app">python_app</option>
        <option value="node_minimal">node_minimal</option>
      </select>
    </div>
    <div>
      <label for="mode">Modo</label>
      <select id="mode">
        <option value="auto" selected>auto</option>
        <option value="build">build</option>
        <option value="improve">improve</option>
      </select>
    </div>
  </div>
  <label class="hive-check">
    <input type="checkbox" id="force" />
    force_scaffold
  </label>
  <button id="run" type="button">Ejecutar ciclo de La Reina</button>
  <label for="log">Salida</label>
  <pre id="log" class="hive-log"></pre>
  <script src="${scriptUri}"></script>
</body>
</html>`;
}
