/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

// Native hmg + hmc + HiveMQ CE, using the W3C WebDriver API through tauri-driver.
// No mocked IPC, store, broker, or database. Missing prerequisites fail the run.
import assert from "node:assert/strict";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const windows = Deno.build.os === "windows";
const extension = windows ? ".exe" : "";
assert(Deno.build.os === "linux" || windows, "Native WebDriver tests require Linux or Windows.");
assert(Deno.args.every((arg) => arg === "--release"), "Usage: pnpm test:e2e [--release]");
const binaryDirectory = Deno.env.get("HIVEME_E2E_BIN_DIR")
  ?? join(root, "target", Deno.args.includes("--release") ? "release" : "debug");
const hmc = join(binaryDirectory, `hmc${extension}`);
const hmg = join(binaryDirectory, `hmg${extension}`);
const artifacts = join(root, "target", "e2e", crypto.randomUUID());
await Deno.mkdir(artifacts, { recursive: true });
// Config, SQLite files, screenshots, and logs all belong to this run. Keep them for diagnosis.
const configPath = join(artifacts, "HiveMe.json");
const env = {
  HIVEME_CONFIG: configPath,
  HIVEME_PASSWORD: "e2e-password",
  ...(windows ? { TEMP: artifacts, TMP: artifacts } : {}),
};
const decoder = new TextDecoder();
const elementKey = "element-6066-11e4-a52e-4f735466cecf";
type Element = Record<typeof elementKey, string>;
type Row = { body: string; topic: string; level: string; raw: string; outgoing: boolean };
type SavedConfig = {
  topics: { prefix: string; subscriptions: string[]; default?: string };
  gui: { language: string; displayMode: string; window: { size: { width: number; height: number } } };
  notifications: { enabled: boolean };
  update: { lastChecked: number };
};

async function command(executable: string, args: string[], timeout = 30_000): Promise<string> {
  const child = new Deno.Command(executable, {
    args: executable === hmc ? ["--config", configPath, ...args] : args,
    cwd: artifacts, env, stdin: "null", stdout: "piped", stderr: "piped",
  }).spawn();
  const timer = setTimeout(() => { try { child.kill(); } catch { /* Already exited. */ } }, timeout);
  try {
    const output = await child.output();
    const stdout = decoder.decode(output.stdout);
    const stderr = decoder.decode(output.stderr);
    await Deno.writeTextFile(join(artifacts, "commands.log"), `${executable} ${args.join(" ")}\n${stdout}${stderr}\n`, { append: true });
    assert(output.success, `${executable} exited ${output.code}: ${stderr}`);
    return stdout.trim();
  } finally {
    clearTimeout(timer);
  }
}

async function until<T>(description: string, check: () => Promise<T>, timeout = 20_000): Promise<NonNullable<T>> {
  const deadline = Date.now() + timeout;
  let lastError: unknown;
  do {
    try {
      const result = await check();
      if (result) return result as NonNullable<T>;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 150));
  } while (Date.now() < deadline);
  throw new Error(`Timed out waiting for ${description}`, { cause: lastError });
}

function freePort(): number {
  const listener = Deno.listen({ hostname: "127.0.0.1", port: 0 });
  const port = listener.addr.port;
  listener.close();
  return port;
}

const port = freePort();
let nativePort = freePort();
while (nativePort === port) nativePort = freePort();
let session = "";
let container = "";
let driver: Deno.ChildProcess | undefined;
let driverLogs: Promise<void>[] = [];

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(path === "/session" ? 90_000 : 30_000),
  });
  const result = await response.json();
  assert(response.ok && !result.value?.error, `${method} ${path}: ${JSON.stringify(result.value)}`);
  return result.value as T;
}

function execute<T>(script: string, args: unknown[] = []): Promise<T> {
  return request("POST", `/session/${session}/execute/sync`, { script, args });
}

// Read the actual backend to verify persisted wire data, in addition to visible UI assertions.
async function invoke<T>(name: string, args: unknown = {}): Promise<T> {
  const result = await request<{ value: T; error?: string }>("POST", `/session/${session}/execute/async`, {
    script: `const done = arguments[arguments.length - 1];
      window.__TAURI_INTERNALS__.invoke(arguments[0], arguments[1])
        .then(value => done({value}), error => done({error: String(error)}));`,
    args: [name, args],
  });
  assert(!result.error, `${name}: ${result.error}`);
  return result.value;
}

function find(value: string, using = "css selector"): Promise<Element> {
  return until(`element ${value}`, () => request("POST", `/session/${session}/element`, { using, value }));
}

async function click(element: Element): Promise<void> {
  await request("POST", `/session/${session}/element/${element[elementKey]}/click`, {});
}

async function clickText(role: string, text: string): Promise<void> {
  await click(await find(`//*[@role='${role}' and normalize-space(.)='${text}']`, "xpath"));
}

async function readConfig(): Promise<SavedConfig> {
  return JSON.parse(await Deno.readTextFile(configPath));
}

async function rows(topic = "hiveme"): Promise<Row[]> {
  return await invoke("get_messages", { topic, limit: 100 });
}

async function visibleBodies(): Promise<string[]> {
  return await execute("return Array.from(document.querySelectorAll('article .message-bubble'), e => e.textContent.trim());");
}

async function expectVisible(bodies: string[]): Promise<void> {
  await until(`visible messages ${JSON.stringify(bodies)}`, async () => {
    const visible = await visibleBodies();
    return bodies.length === visible.length && bodies.every((body) => visible.includes(body));
  });
}

async function expectStartup(): Promise<void> {
  await until("selected and highlighted hiveme root", () => execute(`
    const selected = document.querySelector('[role="treeitem"][aria-selected="true"]');
    const content = selected?.querySelector('.MuiTreeItem-content[data-selected]');
    return content?.textContent.trim() === 'hiveme'
      && getComputedStyle(content).backgroundColor !== 'rgba(0, 0, 0, 0)';`));
  await until("connected GUI with a subscription", async () => {
    const status = await invoke<{ state: string; subscriptions: number }>("get_status");
    return status.state === "Connected" && status.subscriptions > 0;
  });
}

async function openGui(): Promise<void> {
  console.info("Opening a native WebDriver session.");
  const value = await request<{ sessionId: string }>("POST", "/session", {
    capabilities: {
      alwaysMatch: {
        "tauri:options": {
          application: hmg,
          args: ["--config", configPath],
          ...(windows ? { webviewOptions: { userDataFolder: join(artifacts, "webview") } } : {}),
        },
      },
      firstMatch: [{}],
    },
  });
  session = value.sessionId;
  console.info("Native WebDriver session connected.");
  await request("POST", `/session/${session}/timeouts`, { script: 20_000, implicit: 0 });
  await expectStartup();
}

async function closeGui(): Promise<void> {
  await request("DELETE", `/session/${session}`);
  session = "";
}

async function publish(body: string, options: string[] = [], topic = "hiveme"): Promise<void> {
  const stdout = await command(hmc, [...options, body]);
  assert.equal(stdout, `Message sent to ${topic}.`);
  await until(`stored message ${body}`, async () => (await rows()).some((row) => row.body === body));
}

async function saveScreenshot(name: string): Promise<void> {
  const base64 = await request<string>("GET", `/session/${session}/screenshot`);
  await Deno.writeFile(join(artifacts, name), Uint8Array.from(atob(base64), (c) => c.charCodeAt(0)));
}

try {
  await Deno.stat(hmc);
  await Deno.stat(hmg);
  console.info("Starting an isolated HiveMQ CE broker and native hmg.");
  container = await command("docker", ["run", "--detach", "--rm", "--publish", "127.0.0.1::1883", "hivemq/hivemq-ce:latest"], 180_000);
  const mapping = await command("docker", ["port", container, "1883/tcp"]);
  assert.match(mapping, /^127\.0\.0\.1:\d+$/);
  await until("HiveMQ startup", async () => (await command("docker", ["logs", container])).includes("Started HiveMQ"), 90_000);
  const setup = { v: 1, url: `mqtt://${mapping}`, username: "e2e-user", password: "e2e-password", prefix: "hiveme" };
  const initialized = await command(hmc, ["--init", JSON.stringify(setup)]);
  assert.match(initialized, /^Config has been created:/);
  const config = await readConfig();
  assert.deepEqual(Object.keys(config.topics).sort(), ["prefix", "subscriptions"]);
  config.notifications.enabled = false;
  config.gui.language = "en-US";
  config.gui.displayMode = "Light";
  config.gui.window.size = { width: 1200, height: 1000 };
  config.update.lastChecked = Math.floor(Date.now() / 1000);
  await Deno.writeTextFile(configPath, JSON.stringify(config, null, 2));

  const driverArgs = ["--port", String(port), "--native-port", String(nativePort)];
  const nativeDriver = Deno.env.get("HIVEME_E2E_NATIVE_DRIVER");
  if (nativeDriver) driverArgs.push("--native-driver", nativeDriver);
  driver = new Deno.Command("tauri-driver", { args: driverArgs, cwd: root, env, stdin: "null", stdout: "piped", stderr: "piped" }).spawn();
  for (const [name, stream] of [["driver.stdout.log", driver.stdout], ["driver.stderr.log", driver.stderr]] as const) {
    const file = await Deno.open(join(artifacts, name), { create: true, write: true });
    driverLogs.push(stream.pipeTo(file.writable));
  }
  await until("WebDriver startup", () => request("GET", "/status"));
  await openGui();
  assert.deepEqual(await rows(), []);
  await expectVisible([]);
  console.info("PASS: fresh database starts with hiveme selected and highlighted.");

  await click(await find('[aria-label="Settings (F10)"]'));
  await until("Appearance selected first", () => execute(`
    const tabs = document.querySelectorAll('[role="tablist"][aria-orientation="vertical"] [role="tab"]');
    return tabs[0]?.textContent.trim() === 'Appearance' && tabs[0].getAttribute('aria-selected') === 'true';`));
  await click(await find("//button[normalize-space(.)='Dark Mode']", "xpath"));
  await until("appearance saved automatically", async () => (await readConfig()).gui.displayMode === "Dark");
  await clickText("tab", "Topics");
  assert.equal(await execute("return Array.from(document.querySelectorAll('label'), e => e.textContent).includes('Default topic');"), false);
  assert.equal(Object.hasOwn((await readConfig()).topics, "default"), false);
  await clickText("tab", "Messages");
  console.info("PASS: Appearance opens first, changes save immediately, and there is no default-topic setting.");

  const bodies = ["haha"];
  await publish("haha");
  for (const level of ["debug", "info", "warn", "error"]) {
    const body = `CLI ${level}`;
    bodies.push(body);
    await publish(body, ["--level", level]);
  }
  const received = await rows();
  assert.equal(received.length, 5);
  for (const row of received) {
    const level = row.body === "haha" ? "info" : row.body.slice(4);
    assert.equal(row.topic, "hiveme");
    assert.equal(row.level, level);
    assert.equal(JSON.parse(row.raw).payload.level, level);
    assert.equal(JSON.parse(row.raw).sender.app, "hmc");
  }
  await expectVisible(bodies);
  assert.equal(await execute("return document.querySelectorAll('[role=treeitem]').length;"), 1);
  console.info("PASS: hmc haha and every log level publish to hiveme, print confirmation, and appear in hmg.");

  const composer = await find('textarea[aria-label="Write a message. Enter sends, Shift+Enter adds a line."]');
  await request("POST", `/session/${session}/element/${composer[elementKey]}/value`, { text: "GUI reply" });
  await click(await find('button[aria-label="Send"]'));
  bodies.push("GUI reply");
  await expectVisible(bodies);
  const reply = (await rows()).filter((row) => row.body === "GUI reply");
  assert.equal(reply.length, 1, "The broker echo must not duplicate the outgoing message.");
  assert.equal(reply[0].topic, "hiveme");
  assert.equal(reply[0].outgoing, true);
  console.info("PASS: the initially selected root accepts a GUI message and reconciles its broker echo.");

  await publish("Nested child", ["--topic", "build/ci"], "hiveme/build/ci");
  bodies.push("Nested child");
  await expectVisible(bodies);
  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='build']", "xpath"));
  await expectVisible(["Nested child"]);
  const buildExpanded = () => execute<string>(`return Array.from(document.querySelectorAll('[role="treeitem"]'))
    .find(e => e.querySelector(':scope > .MuiTreeItem-content .MuiTypography-root')?.textContent === 'build')?.getAttribute('aria-expanded');`);
  assert.equal(await buildExpanded(), "true", "Selecting the parent must not collapse its children.");
  const buildIcon = await find("//*[@role='treeitem'][./*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='build']]/*[contains(@class,'MuiTreeItem-content')]/*[contains(@class,'MuiTreeItem-iconContainer')]", "xpath");
  await click(buildIcon);
  assert.equal(await buildExpanded(), "false");
  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='build']", "xpath"));
  assert.equal(await buildExpanded(), "false", "Selecting the parent must not expand its children.");
  await expectVisible(["Nested child"]);
  await click(buildIcon);
  assert.equal(await buildExpanded(), "true");
  assert.deepEqual((await rows("hiveme/build")).map((row) => row.topic), ["hiveme/build/ci"]);
  console.info("PASS: parent selection displays recursive history; only the expansion icon toggles children.");

  await closeGui();
  // An unknown JSON field is ignored, not migrated or interpreted as a publish setting.
  const saved = await readConfig();
  saved.topics.default = "info";
  await Deno.writeTextFile(configPath, JSON.stringify(saved, null, 2));
  await openGui();
  await expectVisible(bodies);
  assert.equal((await rows()).length, bodies.length);
  assert.equal(Object.hasOwn((await invoke<SavedConfig>("get_config")).topics, "default"), false);
  await publish("No configurable suffix");
  bodies.push("No configurable suffix");
  await expectVisible(bodies);
  assert((await rows()).filter((row) => row.topic !== "hiveme").every((row) => row.topic === "hiveme/build/ci"));
  await saveScreenshot("passed.png");
  console.info("PASS: restart selects hiveme, reloads SQLite history, and an unknown config field cannot redirect publishing.");
} catch (error) {
  if (session) {
    try {
      await saveScreenshot("failed.png");
      await Deno.writeTextFile(join(artifacts, "failed.html"), await execute("return document.documentElement.outerHTML;"));
    } catch (captureError) {
      console.error("Could not capture the failure:", captureError);
    }
  }
  throw error;
} finally {
  const cleanupErrors: unknown[] = [];
  if (session) {
    try { await closeGui(); } catch (error) { cleanupErrors.push(error); }
  }
  if (driver) {
    try { driver.kill(); } catch { /* Already exited. */ }
    await driver.status;
    await Promise.all(driverLogs);
  }
  if (container) {
    try { await command("docker", ["rm", "--force", container]); } catch (error) { cleanupErrors.push(error); }
  }
  console.info(`E2E artifacts: ${artifacts}`);
  if (cleanupErrors.length) throw new AggregateError(cleanupErrors, "E2E cleanup failed");
}
