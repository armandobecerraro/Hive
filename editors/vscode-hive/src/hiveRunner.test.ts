import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import {
  buildHiveRequest,
  deriveTitle,
  writeHiveRequestJson,
} from "./hiveRunner";

describe("deriveTitle", () => {
  it("devuelve texto por defecto si está vacío", () => {
    assert.equal(deriveTitle(""), "Misión Hive");
    assert.equal(deriveTitle("   \n"), "Misión Hive");
  });

  it("usa la primera línea", () => {
    assert.equal(deriveTitle("hola\nmundo"), "hola");
  });

  it("trunca a 72 caracteres", () => {
    const long = "x".repeat(80);
    assert.equal(deriveTitle(long).length, 72);
  });
});

describe("buildHiveRequest", () => {
  it("arma el payload y recorta descripción", () => {
    const p = buildHiveRequest("  desc  \n", "python_app", "improve", false);
    assert.equal(p.title, "desc");
    assert.equal(p.description, "desc");
    assert.equal(p.stack, "python_app");
    assert.equal(p.work_mode, "improve");
    assert.equal(p.force_scaffold, false);
  });
});

describe("writeHiveRequestJson", () => {
  it("escribe JSON válido en la raíz del repo", () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "hive-vscode-test-"));
    try {
      const written = writeHiveRequestJson(
        dir,
        buildHiveRequest("misión", "node_minimal", "build", true)
      );
      assert.equal(written, path.join(dir, "hive.request.json"));
      const raw = fs.readFileSync(written, "utf8");
      const j = JSON.parse(raw) as {
        title: string;
        description: string;
        stack: string;
        work_mode: string;
        force_scaffold: boolean;
      };
      assert.equal(j.title, "misión");
      assert.equal(j.stack, "node_minimal");
      assert.equal(j.work_mode, "build");
      assert.equal(j.force_scaffold, true);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});
