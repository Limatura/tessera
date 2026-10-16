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
    alias: { fd: "docs: fix typos" },
    messages: {
      type: "Select the type of change:",
      scope: "Select a scope (optional):",
      customScope: "Enter a custom scope:",
      subject: "Write a short description of the change:\n",
      body: 'A longer description (optional). Use "|" for a new line:\n',
      breaking:
        'List any breaking changes (optional). Use "|" for a new line:\n',
      footerPrefixesSelect:
        "Select how the change relates to an issue (optional):",
      customFooterPrefix: "Enter a custom issue prefix:",
      footer: "List related issues (optional), e.g. #31, #42:\n",
      confirmCommit: "Commit, or edit the message?",
    },
    types: [
      { value: "feat", name: "feat:     A new feature" },
      { value: "fix", name: "fix:      A bug fix" },
      {
        value: "docs",
        name: "docs:     Documentation only changes",
      },
      {
        value: "style",
        name: "style:    Changes that do not affect the meaning of the code",
      },
      {
        value: "refactor",
        name: "refactor: A code change that neither fixes a bug nor adds a feature",
      },
      {
        value: "perf",
        name: "perf:     A code change that improves performance",
      },
      {
        value: "test",
        name: "test:     Adding missing tests or correcting existing tests",
      },
      {
        value: "build",
        name: "build:    Changes that affect the build system or external dependencies",
      },
      {
        value: "ci",
        name: "ci:       Changes to our CI configuration files and scripts",
      },
      { value: "revert", name: "revert:   Revert to a commit" },
      {
        value: "chore",
        name: "chore:    Other changes that do not modify src or test files",
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
      { value: "link", name: "link:     Refers to an issue still in progress" },
      { value: "closed", name: "closed:   Closes an issue" },
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
