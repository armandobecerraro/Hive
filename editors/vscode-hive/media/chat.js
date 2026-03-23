(function () {
  const vscode = acquireVsCodeApi();
  const desc = document.getElementById("desc");
  const stack = document.getElementById("stack");
  const mode = document.getElementById("mode");
  const force = document.getElementById("force");
  const runBtn = document.getElementById("run");
  const logEl = document.getElementById("log");

  function appendLine(text, channel) {
    const span = document.createElement("span");
    span.className = channel === "stderr" ? "hive-stderr" : "hive-stdout";
    span.textContent = text;
    logEl.appendChild(span);
    logEl.scrollTop = logEl.scrollHeight;
  }

  runBtn.addEventListener("click", () => {
    vscode.postMessage({
      type: "run",
      payload: {
        description: desc.value,
        stack: stack.value,
        work_mode: mode.value,
        force_scaffold: force.checked,
      },
    });
  });

  window.addEventListener("message", (event) => {
    const msg = event.data;
    if (!msg || typeof msg.type !== "string") {
      return;
    }
    if (msg.type === "runStart") {
      logEl.textContent = "";
      runBtn.disabled = true;
      return;
    }
    if (msg.type === "runEnd") {
      runBtn.disabled = false;
      return;
    }
    if (msg.type === "log" && msg.payload) {
      const ch = msg.payload.channel === "stderr" ? "stderr" : "stdout";
      appendLine(msg.payload.text, ch);
      return;
    }
    if (msg.type === "done" && msg.payload) {
      appendLine(
        `\n[proceso terminado: código ${msg.payload.code}]\n`,
        "stdout"
      );
      return;
    }
    if (msg.type === "error" && msg.payload && msg.payload.message) {
      appendLine("\nError: " + msg.payload.message + "\n", "stderr");
    }
  });
})();
