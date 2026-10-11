import fs from "node:fs/promises";

import { defineConfig } from "czg";

const getDirectories = async (source: string) => {
  const directories = await fs.readdir(source, { withFileTypes: true });

  return directories
    .filter((dirent) => dirent.isDirectory())
    .map((dirent) => dirent.name);
};

const apps = await getDirectories(`${import.meta.dirname}/apps`);
const packages = await getDirectories(`${import.meta.dirname}/packages`);

const scopes = [...apps, ...packages];

export default defineConfig({
  extends: ["@commitlint/config-conventional"],
  rules: {
    "scope-enum": [2, "always", ["release", ...scopes]],
  },
  prompt: {
    alias: { fd: "docs: 修正錯字" },
    messages: {
      type: "選擇你要提交的類型 :",
      scope: "選擇一個提交範圍（可選）:",
      customScope: "請輸入自訂的提交範圍 :",
      subject: "填寫簡短精煉的變更描述 :\n",
      body: '填寫更加詳細的變更描述（可選）。使用 "|" 換行 :\n',
      breaking: '列舉非相容性重大變更（可選）。使用 "|" 換行 :\n',
      footerPrefixesSelect: "選擇關聯 issue 前綴（可選）:",
      customFooterPrefix: "輸入自訂 issue 前綴 :",
      footer: "列舉關聯 issue (可選)，例如: #31, #I3244 :\n",
      confirmCommit: "是否提交或修改 commit ?",
    },
    types: [
      { value: "feat", name: "feat:     新增功能 | A new feature" },
      { value: "fix", name: "fix:      修正缺陷 | A bug fix" },
      {
        value: "docs",
        name: "docs:     文件更新 | Documentation only changes",
      },
      {
        value: "style",
        name: "style:    代碼格式 | Changes that do not affect the meaning of the code",
      },
      {
        value: "refactor",
        name: "refactor: 代碼重構 | A code change that neither fixes a bug nor adds a feature",
      },
      {
        value: "perf",
        name: "perf:     性能提升 | A code change that improves performance",
      },
      {
        value: "test",
        name: "test:     測試相關 | Adding missing tests or correcting existing tests",
      },
      {
        value: "build",
        name: "build:    構建相關 | Changes that affect the build system or external dependencies",
      },
      {
        value: "ci",
        name: "ci:       持續集成 | Changes to our CI configuration files and scripts",
      },
      { value: "revert", name: "revert:   回退代碼 | Revert to a commit" },
      {
        value: "chore",
        name: "chore:    其他修改 | Other changes that do not modify src or test files",
      },
    ],
    useEmoji: false,
    emojiAlign: "center",
    useAI: false,
    aiNumber: 1,
    themeColorCode: "",
    scopes,
    enableMultipleScopes: true,
    scopeEnumSeparator: ",",
    allowCustomScopes: true,
    allowEmptyScopes: true,
    customScopesAlign: "bottom",
    customScopesAlias: "custom",
    emptyScopesAlias: "empty",
    upperCaseSubject: false,
    markBreakingChangeMode: false,
    allowBreakingChanges: ["feat", "fix"],
    breaklineNumber: 100,
    breaklineChar: "|",
    skipQuestions: [],
    issuePrefixes: [
      { value: "link", name: "link:     連結 ISSUES 進行中" },
      { value: "closed", name: "closed:   標記 ISSUES 已完成" },
    ],
    customIssuePrefixAlign: "top",
    emptyIssuePrefixAlias: "skip",
    customIssuePrefixAlias: "custom",
    allowCustomIssuePrefix: true,
    allowEmptyIssuePrefix: true,
    confirmColorize: true,
    scopeOverrides: undefined,
    defaultBody: "",
    defaultIssues: "",
    defaultScope: "",
    defaultSubject: "",
  },
});
