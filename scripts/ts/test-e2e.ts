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
type Row = { body: string; topic: string; level: string; raw: string; outgoing: boolean; qos: number; retain: boolean };
type SavedConfig = {
  topics: { subscriptions: string[]; default?: string; prefix?: string };
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

async function typeInto(element: Element, text: string): Promise<void> {
  await until("editable input", () => request<boolean>("GET", `/session/${session}/element/${element[elementKey]}/enabled`));
  await request("POST", `/session/${session}/element/${element[elementKey]}/clear`, {});
  await request("POST", `/session/${session}/element/${element[elementKey]}/value`, { text });
}

function optionField(label: string): Promise<Element> {
  return find(`//input[@id=//label[normalize-space(.)='${label}']/@for]`, "xpath");
}

async function toggleOptions(): Promise<void> {
  await click(await find("//button[normalize-space(.)='More Options']", "xpath"));
  await until("options panel animation", () => execute(`
    const button = Array.from(document.querySelectorAll('button')).find(e => e.textContent.trim() === 'More Options');
    const panel = document.getElementById(button.getAttribute('aria-controls'));
    return button.getAttribute('aria-expanded') === 'true'
      ? panel?.classList.contains('MuiCollapse-entered')
      : !panel;`));
}

async function expectComposerLayout(expanded: boolean): Promise<void> {
  await until(`${expanded ? "expanded" : "collapsed"} composer layout`, () => execute(`
    const input = document.querySelector('textarea[aria-label]');
    const field = input.closest('.MuiTextField-root');
    const container = field.parentElement.parentElement;
    const more = Array.from(container.querySelectorAll('button')).find(e => e.textContent.trim() === 'More Options');
    const send = container.querySelector('button[aria-label="Send"]');
    const level = container.querySelector('[role="combobox"][aria-label="Level"]').getBoundingClientRect();
    // Measure against the visible panel border, including every surrounding wrapper.
    let panel = container.parentElement;
    while (panel && parseFloat(getComputedStyle(panel).borderRightWidth) === 0) panel = panel.parentElement;
    if (!panel) return false;
    const frame = panel.getBoundingClientRect();
    const frameStyle = getComputedStyle(panel);
    const rect = field.getBoundingClientRect();
    const bounds = container.getBoundingClientRect();
    const actions = send.getBoundingClientRect();
    const toggle = more.getBoundingClientRect();
    const close = (actual, expected) => Math.abs(actual - expected) < 1;
    const checks = Array.from(container.querySelectorAll('input[type="checkbox"]'), e => e.closest('label').getBoundingClientRect());
    const radios = container.querySelector('[role="radiogroup"]')?.getBoundingClientRect();
    const lastBottom = arguments[0] ? Math.max(radios?.bottom ?? 0, ...checks.map(r => r.bottom)) : actions.bottom;
    const valid = more.getAttribute('aria-expanded') === String(arguments[0])
      && close(rect.left - bounds.left, 4)
      && close(rect.top - bounds.top - parseFloat(getComputedStyle(container).borderTopWidth), 4)
      && close(frame.right - parseFloat(frameStyle.borderRightWidth) - rect.right, 4)
      && close(frame.bottom - parseFloat(frameStyle.borderBottomWidth) - lastBottom, 4)
      // Native textarea autosizing rounds each line's height to whole pixels.
      && close(input.getBoundingClientRect().height / 3, parseFloat(getComputedStyle(input).lineHeight))
      && close(actions.top - rect.bottom, 4)
      && close(actions.right, rect.right)
      && close(actions.top, toggle.top) && toggle.right < actions.left
      && level.right < toggle.left && close((level.top + level.bottom) / 2, (actions.top + actions.bottom) / 2)
      && (!arguments[0] || (checks.length === 2 && checks.every(r =>
        close((r.top + r.bottom) / 2, (radios.top + radios.bottom) / 2) && r.left >= radios.right)));
    if (!valid) throw new Error(JSON.stringify({
      expanded: more.getAttribute('aria-expanded'),
      left: rect.left - bounds.left,
      top: rect.top - bounds.top - parseFloat(getComputedStyle(container).borderTopWidth),
      right: frame.right - parseFloat(frameStyle.borderRightWidth) - rect.right,
      bottom: frame.bottom - parseFloat(frameStyle.borderBottomWidth) - lastBottom,
      inputHeight: input.getBoundingClientRect().height,
      lineHeight: getComputedStyle(input).lineHeight,
      actionGap: actions.top - rect.bottom,
      actionAlignment: actions.right - rect.right,
      toggleAlignment: actions.top - toggle.top,
      radios, checks,
    }));
    return valid;`, [expanded]));
}

async function optionValues() {
  return await execute<{
    topic: string; title: string; level: string; qos: string; retain: boolean; json: boolean;
  }>(`
    const labels = Array.from(document.querySelectorAll('label'));
    const field = text => document.getElementById(labels.find(e => e.textContent.trim() === text)?.htmlFor);
    const checked = text => labels.find(e => e.textContent.trim() === text)?.querySelector('input')?.checked;
    return {
      topic: field('Topic')?.value, title: field('Title')?.value,
      level: document.querySelector('input[name="composer-level"]')?.value,
      qos: document.querySelector('input[type="radio"]:checked')?.value,
      retain: checked('Retain Message'), json: checked('As Raw JSON')
    };`);
}

const defaultOptions = { topic: "", title: "", level: "info", qos: "config", retain: false, json: false };

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

async function expectMessageMetadata(body: string, relativeTopic: string, hasSender: boolean): Promise<void> {
  await until(`sender and topic for ${body}`, () => execute(`
    const article = Array.from(document.querySelectorAll('article')).find(e => e.querySelector('.message-bubble')?.textContent.trim() === arguments[0]);
    if (!article) return false;
    const header = article.querySelector('header');
    const bubble = article.querySelector('.message-bubble').getBoundingClientRect();
    const topic = article.querySelector('footer .message-topic');
    const level = article.querySelector('.message-controls .MuiChip-root');
    const topicBox = topic?.getBoundingClientRect();
    const levelBox = level.getBoundingClientRect();
    return (topic?.textContent ?? '') === arguments[1]
      && Boolean(header) === arguments[2]
      && (!header || (header.children.length === 1 && getComputedStyle(header).opacity === '1'
        && header.getBoundingClientRect().bottom <= bubble.top))
      && (!topic || (topic.nextElementSibling === level
        && topic.parentElement === article.querySelector('.message-controls')
        && topicBox.top >= bubble.bottom && topicBox.right <= levelBox.left
        && Math.abs(topicBox.top + topicBox.height / 2 - levelBox.top - levelBox.height / 2) < 2));`,
    [body, relativeTopic, hasSender]));
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
  const setup = { v: 1, url: `mqtt://${mapping}`, username: "e2e-user", password: "e2e-password" };
  const initialized = await command(hmc, ["--init", JSON.stringify(setup)]);
  assert.match(initialized, /^Config has been created:/);
  const config = await readConfig();
  assert.deepEqual(Object.keys(config.topics), ["subscriptions"]);
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
  assert.equal(Object.hasOwn((await readConfig()).topics, "prefix"), false);
  assert.equal(await execute("return Array.from(document.querySelectorAll(\'label\'), e => e.textContent).includes(\'Prefix\');"), false);
  const brokerInit = JSON.parse(await invoke<string>("get_broker_init"));
  assert.equal(Object.hasOwn(brokerInit, "prefix"), false);
  assert.equal(brokerInit.language, (await readConfig()).gui.language, "Copy CLI setup carries the window's language.");
  await clickText("tab", "Messages");
  console.info("PASS: Appearance opens first, changes save immediately, and there is no default-topic setting.");

  const bodies = ["haha"];
  await publish("haha");
  for (const level of ["DEBUG", "Info", "sUcCeSs", "WARN", "error"]) {
    const body = `CLI ${level.toLowerCase()}`;
    bodies.push(body);
    await publish(body, ["--level", level]);
  }
  const received = await rows();
  assert.equal(received.length, 6);
  for (const row of received) {
    const level = row.body === "haha" ? "info" : row.body.slice(4);
    assert.equal(row.topic, "hiveme");
    assert.equal(row.level, level);
    assert.equal(JSON.parse(row.raw).payload.level, level);
    assert.equal(JSON.parse(row.raw).sender.app, "hmc");
  }
  await expectVisible(bodies);
  assert(await execute(`
    const articles = Array.from(document.querySelectorAll('article'));
    const success = articles.find(e => e.querySelector('.message-bubble')?.textContent.trim() === 'CLI success');
    const info = articles.find(e => e.querySelector('.message-bubble')?.textContent.trim() === 'CLI info');
    return success.querySelector('.MuiChip-colorSuccess')?.textContent === 'Success'
      && getComputedStyle(success.querySelector('.message-bubble')).backgroundColor
        !== getComputedStyle(info.querySelector('.message-bubble')).backgroundColor;`), "Success must use the MUI success color.");
  assert.equal(await execute("return document.querySelectorAll('[role=treeitem]').length;"), 1);
  console.info("PASS: hmc haha and every log level publish to hiveme, print confirmation, and appear in hmg.");

  const composer = await find('textarea[aria-label="Write a message. Enter sends, Shift+Enter adds a line."]');
  await request("POST", `/session/${session}/element/${composer[elementKey]}/value`, { text: "GUI reply" });
  await click(await find('button[aria-label="Send"]'));
  bodies.push("GUI reply");
  await expectVisible(bodies);
  await until("GUI send completion", () => execute(`
    const input = document.querySelector('textarea[aria-label^="Write a message."]');
    return input.value === '' && !input.disabled;`));
  const reply = (await rows()).filter((row) => row.body === "GUI reply");
  assert.equal(reply.length, 1, "The broker echo must not duplicate the outgoing message.");
  assert.equal(reply[0].topic, "hiveme");
  assert.equal(reply[0].outgoing, true);
  assert.equal(reply[0].retain, false);
  console.info("PASS: the initially selected root accepts a GUI message and reconciles its broker echo.");
  await expectMessageMetadata("GUI reply", "", false);

  const messageArticle = await find("//article[.//*[contains(@class,'message-bubble') and normalize-space(.)='GUI reply']]", "xpath");
  await until("message controls hidden at rest", () => execute(`
    const article = Array.from(document.querySelectorAll('article')).find(e => e.querySelector('.message-bubble')?.textContent.trim() === 'GUI reply');
    return getComputedStyle(article.querySelector('.message-controls')).opacity === '0';`));
  await request("POST", `/session/${session}/actions`, { actions: [{
    type: "pointer", id: "mouse", parameters: { pointerType: "mouse" },
    actions: [{ type: "pointerMove", duration: 0, origin: messageArticle, x: 0, y: 0 }],
  }] });
  await until("message hover metadata and split copy button", () => execute(`
    const article = Array.from(document.querySelectorAll('article')).find(e => e.querySelector('.message-bubble')?.textContent.trim() === 'GUI reply');
    const controls = article.querySelector('.message-controls');
    const level = controls.querySelector('.MuiChip-root');
    const qos = Array.from(controls.querySelectorAll('span')).find(e => e.textContent.trim() === arguments[0]);
    const time = controls.querySelector('time');
    const buttons = controls.querySelector('.MuiButtonGroup-root');
    const items = [level, qos, time, buttons];
    if (getComputedStyle(controls).opacity !== '1' || level?.textContent !== 'Info' || items.some(e => !e)) return false;
    const bounds = items.map(e => e.getBoundingClientRect());
    return bounds.every((r, index) => !index || r.left > bounds[index - 1].right)
      && bounds.every(r => Math.abs((r.top + r.bottom) / 2 - (bounds[0].top + bounds[0].bottom) / 2) < 1)
      && Math.abs(bounds[3].right - article.querySelector('.message-bubble').getBoundingClientRect().right) < 1
      && buttons.querySelector('button[aria-label="Copy"]') && buttons.querySelector('button[aria-label="Copy options"]');`, [`QoS ${reply[0].qos}`]));
  await click(await find("//article[.//*[contains(@class,'message-bubble') and normalize-space(.)='GUI reply']]//button[@aria-label='Copy options']", "xpath"));
  await until("copy dropdown", () => execute(`
    return JSON.stringify(Array.from(document.querySelectorAll('[role="menuitem"]'), e => e.textContent.trim())) === JSON.stringify(['Copy', 'Copy Raw JSON']);`));
  await saveScreenshot("message-controls.png");
  await request("POST", `/session/${session}/actions`, { actions: [{
    type: "key", id: "keyboard",
    actions: [{ type: "keyDown", value: "\uE00C" }, { type: "keyUp", value: "\uE00C" }],
  }] });
  await until("copy dropdown closed", () => execute("return !document.querySelector('[role=menu]');"));
  console.info("PASS: hovering reveals level, QoS, time, and the split button with both copy actions.");

  await publish("Nested child", ["--topic", "build/ci"], "hiveme/build/ci");
  bodies.push("Nested child");
  await expectVisible(bodies);
  await expectMessageMetadata("Nested child", "build/ci", true);
  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='build']", "xpath"));
  await expectVisible(["Nested child"]);
  await expectMessageMetadata("Nested child", "ci", true);
  const buildExpanded = () => execute<string>(`return Array.from(document.querySelectorAll('[role="treeitem"]'))
    .find(e => e.querySelector(':scope > .MuiTreeItem-content .MuiTypography-root')?.textContent === 'build')?.getAttribute('aria-expanded');`);
  // The group mounts and unmounts through a Collapse, whose children cannot be clicked mid-animation.
  const buildGroupSettled = (expanded: boolean) => until(`build group ${expanded ? "expanded" : "collapsed"}`, () => execute(`
    const item = Array.from(document.querySelectorAll('[role="treeitem"]'))
      .find(e => e.querySelector(':scope > .MuiTreeItem-content .MuiTypography-root')?.textContent === 'build');
    const group = item?.querySelector(':scope > [role="group"]');
    return arguments[0] ? group?.classList.contains('MuiCollapse-entered') : item && !group;`, [expanded]));
  assert.equal(await buildExpanded(), "true", "Selecting the parent must not collapse its children.");
  const buildIcon = await find("//*[@role='treeitem'][./*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='build']]/*[contains(@class,'MuiTreeItem-content')]/*[contains(@class,'MuiTreeItem-iconContainer')]", "xpath");
  await click(buildIcon);
  assert.equal(await buildExpanded(), "false");
  await buildGroupSettled(false);
  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='build']", "xpath"));
  assert.equal(await buildExpanded(), "false", "Selecting the parent must not expand its children.");
  await expectVisible(["Nested child"]);
  await click(buildIcon);
  assert.equal(await buildExpanded(), "true");
  await buildGroupSettled(true);
  assert.deepEqual((await rows("hiveme/build")).map((row) => row.topic), ["hiveme/build/ci"]);
  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='ci']", "xpath"));
  await expectMessageMetadata("Nested child", "", true);
  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='build']", "xpath"));
  await expectMessageMetadata("Nested child", "ci", true);
  console.info("PASS: parent selection displays recursive history; only the expansion icon toggles children.");

  const composerConfig = await readConfig();
  await toggleOptions();
  assert.deepEqual(await optionValues(), defaultOptions);
  await publish("CLI leading slash", ["--topic", "/build/ci"], "hiveme/build/ci");
  bodies.push("CLI leading slash");
  await typeInto(await optionField("Topic"), "/ci");
  assert.equal((await optionValues()).topic, "ci");
  await typeInto(await optionField("Title"), "Composer title");
  await click(await find('[role="combobox"][aria-label="Level"]'));
  assert.deepEqual(await execute("return Array.from(document.querySelectorAll('[role=option]'), e => e.textContent.trim());"), ["Info", "Error", "Success", "Warn"]);
  const successOptionColor = await execute<string>(`
    return getComputedStyle(Array.from(document.querySelectorAll('[role=option]'))
      .find(e => e.textContent.trim() === 'Success')).color;`);
  await clickText("option", "Success");
  await until("level menu closed", () => execute("return !document.querySelector('[role=listbox]');"));
  assert.equal(await execute("return getComputedStyle(document.querySelector('[role=combobox][aria-label=Level]')).color;"), successOptionColor);
  await click(await find('input[type="radio"][value="2"]'));
  await click(await find("//label[normalize-space(.)='Retain Message']//input", "xpath"));
  await typeInto(await find('textarea[aria-label^="Write a message."]'), "Composer override");
  await expectComposerLayout(true);
  await saveScreenshot("composer-expanded.png");
  await toggleOptions();
  await expectComposerLayout(false);
  await click(await find('button[aria-label="Send"]'));
  await until("collapsed composer overrides published", async () => {
    const row = (await rows("hiveme/build")).find(row => row.body === "Composer override" && row.outgoing);
    return row && row.topic === "hiveme/build/ci" && row.qos === 2 && row.retain
      && row.level === "success" && JSON.parse(row.raw).payload.level === "success"
      && JSON.parse(row.raw).payload.title === "Composer title";
  });
  bodies.push("Composer titleComposer override");
  await expectMessageMetadata("Composer titleComposer override", "ci", false);
  const childArticle = await find("//article[.//*[contains(@class,'message-bubble') and normalize-space(.)='Nested child']]", "xpath");
  const childRowHasOpacity = (opacity: string) => execute(`
    const article = Array.from(document.querySelectorAll('article')).find(e => e.querySelector('.message-bubble')?.textContent.trim() === 'Nested child');
    const controls = article.querySelector('.message-controls');
    return controls.contains(article.querySelector('.message-topic'))
      && getComputedStyle(controls).opacity === arguments[0];`, [opacity]);
  await until("entire child message row hidden at rest", () => childRowHasOpacity("0"));
  await request("POST", `/session/${session}/actions`, { actions: [{
    type: "pointer", id: "mouse", parameters: { pointerType: "mouse" },
    actions: [{ type: "pointerMove", duration: 0, origin: childArticle, x: 0, y: 0 }],
  }] });
  await until("entire child message row visible on hover", () => childRowHasOpacity("1"));
  await expectMessageMetadata("Nested child", "ci", true);
  await saveScreenshot("message-topics.png");
  await request("POST", `/session/${session}/actions`, { actions: [{
    type: "pointer", id: "mouse", parameters: { pointerType: "mouse" },
    actions: [{ type: "pointerMove", duration: 0, origin: "viewport", x: 0, y: 0 }],
  }] });
  await until("entire child message row hidden after pointer leaves", () => childRowHasOpacity("0"));
  await saveScreenshot("message-topics-hidden.png");
  const buildOptions = { topic: "ci", title: "Composer title", level: "success", qos: "2", retain: true, json: false };

  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='hiveme']", "xpath"));
  await toggleOptions();
  assert.deepEqual(await optionValues(), defaultOptions, "A previously unchanged topic keeps its own defaults.");
  await typeInto(await optionField("Topic"), "/raw");
  await typeInto(await optionField("Title"), "Unused title");
  await click(await find('[role="combobox"][aria-label="Level"]'));
  await clickText("option", "Warn");
  await until("level menu closed", () => execute("return !document.querySelector('[role=listbox]');"));
  await click(await find('input[type="radio"][value="0"]'));
  await click(await find("//label[normalize-space(.)='As Raw JSON']//input", "xpath"));
  const rawBody = "Raw JSON from composer";
  const raw = JSON.stringify({
    ...JSON.parse(reply[0].raw), id: crypto.randomUUID(),
    payload: { level: "info", body: rawBody },
  });
  await typeInto(await find('textarea[aria-label^="Write a JSON payload."]'), raw);
  await toggleOptions();
  await click(await find('button[aria-label="Send"]'));
  await until("raw JSON published with the panel collapsed", async () => {
    const [row] = await rows("hiveme/raw");
    return row?.raw === raw && row.outgoing && row.qos === 0 && !row.retain;
  });

  bodies.push(rawBody);
  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='build']", "xpath"));
  await toggleOptions();
  assert.deepEqual(await optionValues(), buildOptions, "Returning to a topic restores every saved option after sending.");
  assert.equal(await execute("return document.querySelector('textarea[aria-label]').value;"), "");
  await click(await find("//*[@role='treeitem']/*[contains(@class,'MuiTreeItem-content')]//*[normalize-space(.)='hiveme']", "xpath"));
  await toggleOptions();
  assert.deepEqual(await optionValues(), { topic: "raw", title: "Unused title", level: "warn", qos: "0", retain: false, json: true });
  await clickText("tab", "Settings");
  await clickText("tab", "Messages");
  assert.deepEqual(await optionValues(), { topic: "raw", title: "Unused title", level: "warn", qos: "0", retain: false, json: true });
  assert.deepEqual(await readConfig(), composerConfig, "Composer options must not change the config file.");
  await toggleOptions();
  await expectComposerLayout(false);
  await saveScreenshot("composer-collapsed.png");
  console.info("PASS: composer layout, collapsed overrides, per-topic restoration, raw JSON, and config isolation.");

  await closeGui();
  // An unknown JSON field is ignored, not migrated or interpreted as a publish setting.
  const saved = await readConfig();
  saved.topics.default = "info";
  saved.topics.prefix = "elsewhere";
  await Deno.writeTextFile(configPath, JSON.stringify(saved, null, 2));
  await openGui();
  assert.equal(await execute(`return Array.from(document.querySelectorAll('button'))
    .find(e => e.textContent.trim() === 'More Options')?.getAttribute('aria-expanded');`), "false");
  await toggleOptions();
  assert.deepEqual(await optionValues(), defaultOptions, "Composer options must not survive a restart.");
  await toggleOptions();
  await expectVisible(bodies);
  assert.equal((await rows()).length, bodies.length);
  assert.equal(Object.hasOwn((await invoke<SavedConfig>("get_config")).topics, "default"), false);
  assert.equal(Object.hasOwn((await invoke<SavedConfig>("get_config")).topics, "prefix"), false);
  await publish("No configurable suffix");
  bodies.push("No configurable suffix");
  await expectVisible(bodies);
  assert((await rows()).filter((row) => row.topic !== "hiveme").every((row) => ["hiveme/build/ci", "hiveme/raw"].includes(row.topic)));
  await saveScreenshot("passed.png");
  console.info("PASS: restart selects hiveme, reloads SQLite history, and an unknown config field cannot redirect publishing.");

  await toggleOptions();
  const composerControls = [
    ["message", 'textarea[aria-label^="Write a message."]', "css selector"],
    ["Level", '[role="combobox"][aria-label="Level"]', "css selector"],
    ["Topic", "//input[@id=//label[normalize-space(.)='Topic']/@for]", "xpath"],
    ["Title", "//input[@id=//label[normalize-space(.)='Title']/@for]", "xpath"],
    ...["config", "0", "1", "2"].map(value => [`QoS ${value}`, `input[type="radio"][value="${value}"]`, "css selector"]),
    ["Retain Message", "//label[normalize-space(.)='Retain Message']//input", "xpath"],
    ["As Raw JSON", "//label[normalize-space(.)='As Raw JSON']//input", "xpath"],
    ["More Options", "//button[normalize-space(.)='More Options']", "xpath"],
    ["Send", 'button[aria-label="Send"]', "css selector"],
  ];
  for (const [name, selector, using] of composerControls) {
    const body = `Enter from ${name}`;
    await typeInto(await find('textarea[aria-label^="Write a message."]'), body);
    const control = await find(selector, using);
    assert(await execute("arguments[0].focus(); return document.activeElement === arguments[0];", [control]), `${name} must have focus.`);
    await request("POST", `/session/${session}/actions`, { actions: [{
      type: "key", id: "keyboard",
      actions: [{ type: "keyDown", value: "\uE007" }, { type: "keyUp", value: "\uE007" }],
    }] });
    await until(`Enter sends from ${name}`, async () => {
      const sent = (await rows()).filter(row => row.body === body);
      return sent.length === 1 && sent[0].outgoing && sent[0].topic === "hiveme"
        && await execute("return document.querySelector('textarea[aria-label]').value === ''; ");
    });
    assert.deepEqual(await optionValues(), defaultOptions, `Enter must not change ${name}.`);
    assert.equal(await execute(`return Array.from(document.querySelectorAll('button'))
      .find(e => e.textContent.trim() === 'More Options').getAttribute('aria-expanded');`), "true");
  }
  console.info("PASS: Enter sends once from every focused composer control without changing its options.");
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
