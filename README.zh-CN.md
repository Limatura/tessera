<p align="center">
  <img src="apps/web/public/favicon.svg" alt="" width="88" height="88" />
</p>

<h1 align="center">Tessera</h1>

<p align="center">自托管的文件与文本分享服务，用完会自己清理。</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-TW.md">繁體中文</a> · 简体中文 · <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-blue" alt="License: AGPL-3.0" /></a>
</p>

https://github.com/user-attachments/assets/f1c95b78-8561-4184-9616-c57e3fa3bc5a

<p align="center">
  <img src="docs/images/screenshot-upload.png" alt="上传页" width="49%" />
  <img src="docs/images/screenshot-share.png" alt="收件人看到的代码分享" width="49%" />
</p>

上传文件、文件夹、一段代码或一个链接，就会得到一个简短的取件码和网址。拿到的人可以打开它，直到它过期，或下载次数达到你设定的上限，之后它就会被删除。

发送东西不需要账号，除非你关闭这个功能。Tessera 是单个可执行文件，网页界面直接内嵌其中；数据存放在 SQLite、PostgreSQL 或 MySQL，文件放在本地磁盘或对象存储。

## 适合谁用

**自己和信任的人用。** 部署在 NAS 或家用服务器上、放在 VPN 后面，在自己的设备之间传文件，或发给朋友。默认值就是为这种情况设计的：开启匿名上传、SQLite 加本地磁盘，其他都不用配置。参考[个人使用的示例配置](tessera.personal.env.example)。

**向陌生人开放的公共实例。** 任何人都能上传，所以这台机器的每一份资源都在被陌生人使用。限制单个文件大小和总容量、缩短默认保留时间、保持关闭注册或改为邀请制，并接入恶意软件扫描。参考[公共实例的示例配置](tessera.public.env.example)。

**团队或组织。** 关闭匿名上传，让大家用公司的 OpenID Connect 登录。用角色控制谁能上传、谁能查看所有人的分享、谁能修改设置，每个账号也可以单独设置容量上限。需要不止一台机器时，可以换成 PostgreSQL 和 S3 兼容存储。

## 功能

### 底层设计

- **大文件可以不经过你的服务器。** 接入 S3 或 Google Cloud Storage 后，上传会直接写入 bucket，普通下载也会直接重定向到 bucket。有密码或下载次数限制的分享一律经过服务器，所以这些限制依然有效。
- **限制在高负载下也守得住。** 每次取件都用一条带条件的数据库更新来计数，两个人同时抢一次性分享，不会两人都拿到。每个请求都会检查期限，查询取件码也有限流。
- **流式传输，不先整个读入内存。** 无论文件多大，内存占用都保持平稳。
- **存储后端可以混用。** 本地磁盘、S3 兼容存储、Azure Blob、Google Cloud Storage 和 WebDAV，可以同时使用多个，在后台添加或停用都无需重启。大文件甚至可以拆分到多个 bucket 存储（默认关闭）。
- **支持 SQLite、PostgreSQL 或 MySQL**，每种数据库都有各自手写的迁移脚本。
- **可以用来开发的 API。** OpenAPI 规范由服务器代码生成，CI 会检查并做模糊测试。命令行工具和 API token 使用与网页界面相同的端点。
- **容量与限制**可以按角色、按账号设置，也能限制所有匿名上传的总量；此外支持断点续传，可选接入恶意软件扫描。

### 使用上

- **期限与下载次数。** 每个分享都有保留时间，也可以限制最多被取几次。
- **密码**，输入正确才能看到内容或下载。
- **预览。** 代码带语法高亮，图片、SVG 和 Mermaid 图表直接显示在页面上。链接粘贴到聊天软件时会生成预览卡片。
- **文件夹与多个文件**，可以打包成一个压缩包下载。
- **需要时再用账号。** 注册可以设置为开放、邀请制或关闭；可以用密码、GitHub、Discord 或任意 OpenID Connect 登录。
- **界面提供英文、繁体中文和日文。**

## 管理后台

<p align="center">
  <img src="docs/images/screenshot-admin.png" alt="后台仪表盘" width="80%" />
</p>

管理所需的一切都在 `/admin`，几乎所有设置修改后下一个请求就生效，无需重启：

- **仪表盘：** 用了多少空间、存在哪里、流量和趋势，以及有风险的设置提醒。
- **用户：** 停用或删除账号、调整角色、设置单个账号的容量上限，也能一次结束某个账号上传的所有分享。
- **分享：** 实例上的所有分享，包括匿名上传的，以及上传者是谁。
- **邀请码、角色与权限。**
- **设置：** 登录方式、上传限制、默认期限、存储后端、电子邮件、扫描、链接预览和外观。

没有浏览器时，同样的设置也可以用命令修改：`tessera-server settings list`、`settings set <key> <value>`。

## 快速开始

使用 Docker，在本仓库的克隆中执行：

```bash
docker compose up -d
```

然后打开 <http://localhost:12923>。首次启动时，服务器会创建一个管理员账号，并把密码打印在日志里，只打印一次：

```bash
docker compose logs tessera | grep password
```

开放到公网之前，请先阅读[公共实例的示例配置](tessera.public.env.example)。匿名上传默认开启，没有设置容量上限的话，任何能访问这台服务器的人都能把磁盘写满。

### 从源码运行

需要 Rust 和 pnpm。

```bash
pnpm install
pnpm --filter web build
pnpm server:dev
```

## 配置

大部分设置都在管理后台。少数必须在服务器启动前确定的，例如端口、数据库地址和数据目录，用环境变量设置：

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=postgres://tessera:secret@localhost/tessera
TESSERA_SERVER__DATA_DIR=/var/lib/tessera
```

有两份带注释的示例可以作为起点：[个人使用](tessera.personal.env.example)和[公共实例](tessera.public.env.example)。使用 systemd 的话有现成的 [unit 文件](contrib/tessera.service)。

### 存储

文件默认存放在本地目录，要使用其他存储后端可以在管理后台添加。

| 存储后端 | 说明 |
| --- | --- |
| 本地磁盘 | 默认 |
| S3 兼容存储 | 已用 MinIO、Garage、RustFS、SeaweedFS、VersityGW 和 s3proxy 测试。提供凭证时，也会测试 AWS S3、Cloudflare R2 和 Backblaze B2。 |
| Azure Blob Storage | 已用 Azurite 测试 |
| Google Cloud Storage | 已内置，但尚未被测试覆盖 |
| WebDAV | 仅支持小文件，上限为 `share.inline_threshold_bytes`（默认 64 KiB） |

## 命令行工具

在 Linux 或 macOS 安装 `tess`。Windows 版在 [releases 页面](https://github.com/Limatura/tessera/releases)；也可以用 `cargo install --path apps/cli` 从源码编译。

```bash
curl -fsSL https://raw.githubusercontent.com/Limatura/tessera/main/scripts/install.sh | sh
```

正式版发布之前，请在后面加上 `-s -- --version <版本>`。

```bash
tess login https://share.example.com
tess send report.pdf --expires 24
tess get https://share.example.com/s/P8D72PSF
echo "hello" | tess send -
tess ls
tess rm P8D72PSF
tess ui
```

`tess ui` 会打开全屏界面，不用输入路径就能找文件、发送，也能管理自己的分享。`tess send` 不带文件时也会打开它。

加上 `--encrypt` 会在内容离开你的电脑之前先加密，密钥是链接中 `#` 后面的部分，服务器永远看不到。

手边没有 CLI？用 `curl` 也可以，会直接打印链接：

```bash
curl -T report.pdf https://share.example.com/
echo "hello" | curl -T - https://share.example.com/
```

选项放在 header：`-H 'X-Share-Expires: 1d'`、`-H 'X-Share-Max-Downloads: 3'`、`-H 'X-Share-Password: …'`。

## 文档

- [使用与部署指南](https://github.com/Limatura/tessera-docs)
- [服务器内部说明](apps/server/README.md)，以及 [docs/](docs/README.md) 中的设计笔记
- [API 规范](openapi.json)，由服务器代码生成

## 参与贡献

请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。安全问题请按照 [SECURITY.md](SECURITY.md) 私下报告，不要提交 issue。

## 许可证

[AGPL-3.0](LICENSE)。如果你修改了 Tessera 并提供给他人使用，必须让他们能获取修改后的源代码。
