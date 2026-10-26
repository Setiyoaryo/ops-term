import { test, expect, type Page } from "@playwright/test";
import path from "node:path";

const MOCK_PATH = path.resolve(process.cwd(), "e2e/tauri-mock.js");

test.beforeEach(async ({ page }) => {
  await page.addInitScript({ path: MOCK_PATH });
  await page.goto("/");
  // App mounted when the quick-connect bar is present.
  await expect(page.getByTestId("quick-connect")).toBeVisible();
});

/** Visible rows of the currently active terminal pane. */
const activeRows = (page: Page) =>
  page.locator('[data-testid="pane"][data-active="true"] .xterm-rows');

async function focusActiveTerminal(page: Page) {
  await page
    .locator('[data-testid="pane"][data-active="true"] .xterm-helper-textarea')
    .click();
}

async function runCommand(page: Page, command: string) {
  await focusActiveTerminal(page);
  await page.keyboard.type(command);
  await page.keyboard.press("Enter");
}

test("opens a local shell and executes a command end-to-end", async ({ page }) => {
  await page.getByTestId("local-shell").click();

  await expect(page.getByTestId("tab")).toHaveCount(1);
  await expect(page.getByTestId("tab").first()).toHaveAttribute(
    "data-tab-state",
    "connected",
  );
  await expect(activeRows(page)).toContainText("OpsTerm local shell");

  // `uname -a` output ("GNU/Linux") never appears in the typed input, so this
  // proves the command was processed, not merely echoed.
  await runCommand(page, "uname -a");
  await expect(activeRows(page)).toContainText("GNU/Linux");

  await expect(page.getByTestId("status")).toContainText("connected");
});

test("keeps two sessions isolated", async ({ page }) => {
  await page.getByTestId("local-shell").click();
  await page.getByTestId("local-shell").click();
  await expect(page.getByTestId("tab")).toHaveCount(2);

  // Second tab is active.
  await runCommand(page, "echo ONLY_SECOND");
  const paneA = page.getByTestId("pane").nth(0);
  const paneB = page.getByTestId("pane").nth(1);
  await expect(paneB.locator(".xterm-rows")).toContainText("ONLY_SECOND");

  // Switch to the first tab; its buffer must not contain the second's output.
  await page.getByTestId("tab").nth(0).click();
  await expect(paneA).toHaveAttribute("data-active", "true");
  await expect(paneA.locator(".xterm-rows")).not.toContainText("ONLY_SECOND");

  await runCommand(page, "echo ONLY_FIRST");
  await expect(paneA.locator(".xterm-rows")).toContainText("ONLY_FIRST");

  // Back to the second tab: untouched by the first tab's activity.
  await page.getByTestId("tab").nth(1).click();
  await expect(paneB.locator(".xterm-rows")).toContainText("ONLY_SECOND");
  await expect(paneB.locator(".xterm-rows")).not.toContainText("ONLY_FIRST");
});

test("quick connect opens an SSH session tab", async ({ page }) => {
  await page.getByTestId("quick-connect").fill("operator@lab.example.net");
  await page.getByTestId("connect").click();

  await expect(page.getByTestId("tab")).toHaveCount(1);
  await expect(page.getByTestId("tab").first()).toHaveAttribute(
    "data-tab-title",
    "operator@lab.example.net",
  );
  await expect(activeRows(page)).toContainText("Connected to lab.example.net");
});

test("rejects a host that looks like an option without opening a session", async ({
  page,
}) => {
  await page.getByTestId("quick-connect").fill("-oProxyCommand=evil");
  await page.getByTestId("connect").click();

  await expect(page.getByRole("alert")).toContainText("must not begin with");
  await expect(page.getByTestId("tab")).toHaveCount(0);
  await expect(page.getByTestId("pane")).toHaveCount(0);
});

test("closes a tab and reaps its session", async ({ page }) => {
  page.on("dialog", (dialog) => dialog.accept());

  await page.getByTestId("local-shell").click();
  await expect(page.getByTestId("tab")).toHaveCount(1);

  await page.getByTestId("tab").first().getByTestId("tab-close").click();

  await expect(page.getByTestId("tab")).toHaveCount(0);
  await expect(page.getByTestId("status")).toContainText("No active session");
});

test("reconnects the active session with Ctrl+Shift+R", async ({ page }) => {
  await page.getByTestId("local-shell").click();
  await runCommand(page, "echo BEFORE_RECONNECT");
  await expect(activeRows(page)).toContainText("BEFORE_RECONNECT");

  await page.keyboard.press("Control+Shift+KeyR");

  await expect(page.getByTestId("status")).toContainText("connected");
  await expect(activeRows(page)).toContainText("$");
});

test("saves a session and connects it from the sidebar", async ({ page }) => {
  await expect(page.getByTestId("session-sidebar")).toBeVisible();

  await page.getByTestId("new-session").click();
  await expect(page.getByTestId("session-editor")).toBeVisible();
  await page.getByTestId("field-name").fill("CORE-JKT-01");
  await page.getByTestId("field-host").fill("core-jkt-01.example.net");
  await page.getByTestId("session-save").click();

  const entry = page.getByTestId("saved-session").first();
  await expect(entry).toContainText("CORE-JKT-01");

  await entry.click();
  await expect(page.getByTestId("tab")).toHaveCount(1);
  await expect(page.getByTestId("tab").first()).toHaveAttribute(
    "data-tab-title",
    "CORE-JKT-01",
  );
  await expect(activeRows(page)).toContainText("Connected to core-jkt-01.example.net");
});

test("runs a per-device login script on connect", async ({ page }) => {
  await page.getByTestId("new-session").click();
  await page.getByTestId("field-name").fill("SW-LAB");
  await page.getByTestId("field-host").fill("sw-lab.example.net");
  await page
    .getByTestId("field-login-script")
    .fill("sendline HELLO_FROM_SCRIPT");
  await page.getByTestId("session-save").click();

  await page.getByTestId("saved-session").first().click();
  await expect(page.getByTestId("tab")).toHaveCount(1);
  // The login script ran: its command reached the session.
  await expect(activeRows(page)).toContainText("HELLO_FROM_SCRIPT");
});

test("terminal right-click menu offers copy/paste actions", async ({ page }) => {
  await page.getByTestId("local-shell").click();
  await page.getByTestId("split-pane").first().click({ button: "right" });
  await expect(page.getByTestId("terminal-menu")).toBeVisible();
  await expect(page.getByTestId("terminal-menu")).toContainText("Paste");
  await expect(page.getByTestId("terminal-menu")).toContainText("Select all");
});

test("multi-line paste is reviewed before sending", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.getByTestId("local-shell").click();
  await page.evaluate(() => navigator.clipboard.writeText("echo ONE\necho TWO"));

  await page.getByTestId("split-pane").first().click({ button: "right" });
  await page.getByRole("menuitem", { name: "Paste" }).click();

  await expect(page.getByTestId("paste-review")).toBeVisible();
  await page.getByTestId("paste-confirm").click();
  await expect(page.getByTestId("paste-review")).toHaveCount(0);
  await expect(activeRows(page)).toContainText("ONE");
});

test("device editor exposes the Backspace mode", async ({ page }) => {
  await page.getByTestId("new-session").click();
  const bs = page.getByTestId("field-backspace");
  await expect(bs).toBeVisible();
  await bs.selectOption("bs");
  await expect(bs).toHaveValue("bs");
});

test("multi-line paste sends CR between lines (no merged commands)", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.getByTestId("local-shell").click();
  await page.evaluate(() =>
    navigator.clipboard.writeText("echo AA11\necho BB22"),
  );

  await page.getByTestId("split-pane").first().click({ button: "right" });
  await page.getByRole("menuitem", { name: "Paste" }).click();
  await page.getByTestId("paste-confirm").click();

  const rows = activeRows(page);
  await expect(rows).toContainText("AA11");
  await expect(rows).toContainText("BB22");
  // If LF were sent instead of CR, the lines would merge into "AA11echo BB22".
  await expect(rows).not.toContainText("AA11echo BB22");
});

test("switching protocol updates the default port", async ({ page }) => {
  await page.getByTestId("new-session").click();
  await expect(page.getByTestId("field-port")).toHaveValue("22");
  await page.getByTestId("field-protocol").selectOption("telnet");
  await expect(page.getByTestId("field-port")).toHaveValue("23");
  await page.getByTestId("field-protocol").selectOption("ssh");
  await expect(page.getByTestId("field-port")).toHaveValue("22");
});

test("stores a tagged credential and references it from a device", async ({ page }) => {
  await page.getByTestId("open-tools").click();
  await page.getByTestId("tools-tab-credentials").click();
  await page.getByTestId("cred-name").fill("OLT-JKT");
  await page.getByPlaceholder("tags (space/comma)").fill("olt jakarta");
  await page.getByTestId("cred-secret").fill("s3cret");
  await page.getByTestId("cred-save").click();

  const row = page.getByTestId("credential-row").first();
  await expect(row).toContainText("OLT-JKT");
  await expect(row).toContainText("#olt");
  await page.getByTestId("tools-close").click();

  // Reference the credential from a new device.
  await page.getByTestId("new-session").click();
  await page.getByTestId("field-name").fill("OLT-05");
  await page.getByTestId("field-host").fill("olt-05.example.net");
  await page.getByTestId("field-credential").selectOption("OLT-JKT");
  await page.getByTestId("session-save").click();
  await expect(page.getByTestId("saved-session").first()).toContainText("OLT-05");
});

test("Save… prefills and stores the current ad-hoc SSH device", async ({ page }) => {
  await page.getByTestId("quick-connect").fill("operator@lab.example.net");
  await page.getByTestId("connect").click();
  await expect(page.getByTestId("tab")).toHaveCount(1);

  await page.getByTestId("save-session").click();
  await expect(page.getByTestId("session-editor")).toBeVisible();
  await expect(page.getByTestId("field-host")).toHaveValue("lab.example.net");
  await page.getByTestId("field-name").fill("LAB-01");
  await page.getByTestId("session-save").click();

  await expect(page.getByTestId("saved-session").first()).toContainText("LAB-01");
  // The live tab is still open and now linked to the saved device.
  await expect(page.getByTestId("tab")).toHaveCount(1);
});

test("filters saved sessions by metadata", async ({ page }) => {
  await page.getByTestId("new-session").click();
  await page.getByTestId("field-name").fill("OLT-05");
  await page.getByTestId("field-host").fill("olt-05.example.net");
  await page.getByTestId("session-save").click();
  await expect(page.getByTestId("saved-session")).toHaveCount(1);

  await page.getByTestId("session-filter").fill("nomatch");
  await expect(page.getByTestId("saved-session")).toHaveCount(0);

  await page.getByTestId("session-filter").fill("olt");
  await expect(page.getByTestId("saved-session")).toHaveCount(1);
});

test("broadcasts a command to all connected sessions via the composer", async ({
  page,
}) => {
  await page.getByTestId("local-shell").click();
  await page.getByTestId("local-shell").click();
  await expect(page.getByTestId("tab")).toHaveCount(2);

  await page.keyboard.press("Alt+KeyG");
  await expect(page.getByTestId("command-composer")).toBeVisible();
  await page.getByTestId("composer-scope").selectOption("all");
  await page.getByTestId("composer-input").fill("echo BROADCAST_OK");
  await page.getByTestId("arm-broadcast").check();
  await page.getByTestId("composer-send").click();

  await expect(page.getByTestId("composer-results").locator("span")).toHaveCount(2);

  // Inactive tabs are hibernated; switching restores their buffer from the
  // snapshot + buffered output, so each pane shows the broadcast.
  await page.getByTestId("tab").nth(0).click();
  await expect(page.getByTestId("pane").nth(0).locator(".xterm-rows")).toContainText(
    "BROADCAST_OK",
  );
  await page.getByTestId("tab").nth(1).click();
  await expect(page.getByTestId("pane").nth(1).locator(".xterm-rows")).toContainText(
    "BROADCAST_OK",
  );
});

test("duplicates the active session into a new tab", async ({ page }) => {
  await page.getByTestId("local-shell").click();
  await expect(page.getByTestId("tab")).toHaveCount(1);
  await page.getByTestId("duplicate-tab").click();
  await expect(page.getByTestId("tab")).toHaveCount(2);
});

test("reorders tabs by dragging", async ({ page }) => {
  await page.getByTestId("local-shell").click();
  await page.getByTestId("quick-connect").fill("reorder.example.net");
  await page.getByTestId("connect").click();
  await expect(page.getByTestId("tab")).toHaveCount(2);

  const tabs = page.getByTestId("tab");
  await tabs.nth(1).dragTo(tabs.nth(0));
  await expect(page.getByTestId("tab").first()).toHaveAttribute(
    "data-tab-title",
    "reorder.example.net",
  );
});

test("splits a tab into two independent panes", async ({ page }) => {
  await page.getByTestId("local-shell").click();
  await expect(page.getByTestId("split-pane")).toHaveCount(1);

  await page.getByTestId("split-right").click();
  await expect(page.getByTestId("split-pane")).toHaveCount(2);

  // Each split pane hosts its own terminal.
  const panes = page.getByTestId("split-pane");
  await expect(panes.nth(0).locator(".xterm")).toHaveCount(1);
  await expect(panes.nth(1).locator(".xterm")).toHaveCount(1);
});

test("runs automation steps against a live session", async ({ page }) => {
  await page.getByTestId("local-shell").click();
  await page.getByTestId("open-tools").click();
  await expect(page.getByTestId("tools-drawer")).toBeVisible();
  await page.getByTestId("tools-tab-automation").click();
  await page.getByTestId("automation-run").click();
  await expect(page.getByTestId("automation-results")).toContainText("send");
  await expect(activeRows(page)).toContainText("OPSTERM_AUTOMATION");
});

test("browses remote files over SFTP", async ({ page }) => {
  await page.getByTestId("new-session").click();
  await page.getByTestId("field-name").fill("FILES-01");
  await page.getByTestId("field-host").fill("files.example.net");
  await page.getByTestId("session-save").click();

  await page.getByTestId("open-tools").click();
  await page.getByTestId("tools-tab-files").click();
  await page.getByTestId("sftp-session").selectOption({ label: "FILES-01" });
  await page.getByTestId("sftp-connect").click();

  await expect(page.getByTestId("sftp-entry")).toHaveCount(2);
  await expect(page.getByTestId("sftp-entry").first()).toContainText("configs");
});

test("saves and runs a runbook against the active session", async ({ page }) => {
  await page.getByTestId("local-shell").click();

  await page.getByTestId("open-tools").click();
  await page.getByTestId("tools-tab-runbooks").click();
  await page.getByTestId("runbook-name").fill("Greet");
  await page.getByTestId("runbook-script").fill("sendline RUNBOOK_HELLO");
  await page.getByTestId("runbook-save").click();
  await page.getByTestId("runbook-run").click();

  await expect(page.getByTestId("runbook-results")).toContainText("send");
  await expect(activeRows(page)).toContainText("RUNBOOK_HELLO");
});

test("applies a vendor profile in the device editor", async ({ page }) => {
  await page.getByTestId("new-session").click();
  await page.getByTestId("field-vendor").selectOption("huawei");
  await expect(page.getByTestId("field-pager")).toHaveValue("huawei");
  await expect(page.getByTestId("field-backspace")).toHaveValue("bs");
});

test("lists session logs in the tools drawer", async ({ page }) => {
  await page.getByTestId("open-tools").click();
  await page.getByTestId("tools-tab-logs").click();
  await expect(page.getByTestId("log-row")).toHaveCount(1);
  await expect(page.getByTestId("log-row").first()).toContainText("SW-01");
});

test("opens the tools drawer tunnel panel", async ({ page }) => {
  await page.getByTestId("open-tools").click();
  await expect(page.getByTestId("tools-drawer")).toBeVisible();
  await expect(page.getByTestId("tools-tab-tunnels")).toBeVisible();
  await expect(page.getByTestId("tools-drawer")).toContainText("No tunnels running");
});

test("selects the native SSH engine in settings", async ({ page }) => {
  await page.getByTestId("open-settings").click();
  await page.getByRole("button", { name: "SSH" }).click();
  const engine = page.getByTestId("ssh-engine");
  await expect(engine).toBeVisible();
  await engine.selectOption("native");
  await expect(engine).toHaveValue("native");
  await page.getByTestId("settings-save").click();
  await expect(page.getByTestId("settings-panel")).toHaveCount(0);
});

test("switches the UI to light theme", async ({ page }) => {
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByTestId("open-settings").click();
  await page.getByRole("button", { name: "Appearance" }).click();
  await page.getByTestId("ui-theme").selectOption("light");
  await page.getByTestId("settings-save").click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
});

test("saves and restores a workspace with an ad-hoc pane", async ({ page }) => {
  page.on("dialog", (dialog) => dialog.accept("WS1"));
  await page.getByTestId("local-shell").click();
  await expect(page.getByTestId("tab")).toHaveCount(1);

  await page.getByTestId("workspace-save").click();
  // Prompt is accepted by the dialog handler; the workspace is stored.
  const wsSelect = page.getByLabel("Workspaces");
  await expect(wsSelect.locator("option")).toHaveCount(2); // placeholder + saved

  await wsSelect.selectOption({ index: 1 });
  await page.getByRole("button", { name: "Restore" }).click();
  // A new local-shell tab is opened by restore.
  await expect(page.getByTestId("tab")).toHaveCount(2);
});

test("creates a command and inserts it into the composer from the button bar", async ({
  page,
}) => {
  await page.getByTestId("sidebar-tab-commands").click();
  await page.getByTestId("new-command").click();
  await page.getByTestId("command-name").fill("Show version");
  await page.getByTestId("command-content").fill("show version");
  await page.getByTestId("command-save").click();

  const button = page.getByTestId("button-bar-item").first();
  await expect(button).toContainText("Show version");
  await button.click();

  await expect(page.getByTestId("command-composer")).toBeVisible();
  await expect(page.getByTestId("composer-input")).toHaveValue("show version");
});
