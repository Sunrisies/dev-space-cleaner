import { test, expect } from "@playwright/test";

const key = "huanj-scan-settings-v1";
const preset = {
  version: 1,
  mode: "custom",
  roots: ["D:\\projects\\saved-app", "E:\\offline-project"],
  kinds: ["node", "python"],
  workers: 2,
};

test("restores custom roots, types and workers, then preserves them across save and reload", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(
    ({ key, preset }) => localStorage.setItem(key, JSON.stringify(preset)),
    { key, preset },
  );
  await page.reload();
  await expect(
    page.getByRole("region", { name: "扫描控制" }).getByText(/^自定义目录/),
  ).toBeVisible();
  await expect(
    page.getByText("2 个目录 · 已保存", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("checkbox", { name: "target", exact: true }),
  ).not.toBeChecked();
  await expect(
    page.getByRole("button", { name: "开始扫描", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "调整范围", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByText(preset.roots[0], { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByText(preset.roots[1], { exact: true }),
  ).toBeVisible();
  await expect(dialog.getByRole("combobox")).toHaveValue("2");
  await page.screenshot({ path: "docs/screenshots/settings-saved.png" });
  await dialog.getByRole("button", { name: /^所有磁盘/ }).click();
  await dialog.getByRole("combobox").selectOption("4");
  await dialog.getByRole("button", { name: "保存设置", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.reload();
  await expect(
    page.getByRole("region", { name: "扫描控制" }).getByText(/^全盘扫描/),
  ).toBeVisible();
  await expect(
    page
      .getByRole("region", { name: "扫描控制" })
      .getByText("已保存", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "调整范围", exact: true }).click();
  await expect(dialog.getByRole("combobox")).toHaveValue("4");
  await dialog.getByRole("button", { name: /^自定义目录/ }).click();
  await expect(
    dialog.getByText(preset.roots[0], { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByText(preset.roots[1], { exact: true }),
  ).toBeVisible();
  await dialog.getByRole("button", { name: "保存设置", exact: true }).click();
  await page.reload();
  await expect(
    page.getByText("2 个目录 · 已保存", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("准备就绪", { exact: true })).toBeVisible();
});

test("reports corrupt saved settings without overwriting them on startup", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate((key) => localStorage.setItem(key, "{corrupt"), key);
  await page.reload();
  await expect(
    page.getByRole("status").filter({ hasText: "读取扫描设置失败" }),
  ).toBeVisible();
  expect(await page.evaluate((key) => localStorage.getItem(key), key)).toBe(
    "{corrupt",
  );
});

test("failed save keeps the previous stored settings and displays an error", async ({
  page,
}) => {
  await page.addInitScript(
    ({ key, preset }) => {
      localStorage.setItem(key, JSON.stringify(preset));
      const original = Storage.prototype.setItem;
      Storage.prototype.setItem = function (k, v) {
        if (k === key)
          throw new DOMException("Unavailable", "QuotaExceededError");
        return original.call(this, k, v);
      };
    },
    { key, preset },
  );
  await page.goto("/");
  await page.getByRole("button", { name: "调整范围", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("combobox").selectOption("8");
  await dialog.getByRole("button", { name: "保存设置", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("无法保存扫描设置");
  expect(
    await page.evaluate((key) => JSON.parse(localStorage.getItem(key)!), key),
  ).toEqual(preset);
  await expect(
    dialog.getByRole("button", { name: "保存设置", exact: true }),
  ).toBeEnabled();
});
