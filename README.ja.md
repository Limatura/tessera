<p align="center">
  <img src="apps/web/public/favicon.svg" alt="" width="88" height="88" />
</p>

<h1 align="center">Tessera</h1>

<p align="center">使い終わったら自動で片付く、セルフホスト型のファイル・テキスト共有。</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.zh-CN.md">简体中文</a> · 日本語
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-blue" alt="License: AGPL-3.0" /></a>
</p>

https://github.com/user-attachments/assets/f1c95b78-8561-4184-9616-c57e3fa3bc5a

<p align="center">
  <img src="docs/images/screenshot-upload.png" alt="アップロード画面" width="49%" />
  <img src="docs/images/screenshot-share.png" alt="受け取った人に見えるコードの共有" width="49%" />
</p>

ファイル、フォルダー、コードの断片、リンクをアップロードすると、短い取得コードと URL が返ってきます。それを受け取った人は、有効期限が切れるか、設定したダウンロード回数に達するまで開けます。その後は削除されます。

送るだけならアカウントは不要です（この機能はオフにもできます）。Tessera は Web 画面を内蔵した単一のバイナリで動き、データは SQLite・PostgreSQL・MySQL に、ファイルはローカルディスクかオブジェクトストレージに保存します。

## 想定する使い方

**自分と信頼できる人だけで使う。** NAS や自宅サーバーで、VPN の内側に置き、自分のデバイス間でファイルを移したり友人に渡したりする使い方です。初期設定はこの用途に合わせてあります。匿名アップロードが有効で、SQLite とローカルディスクを使い、ほかに設定するものはありません。[個人向けの設定例](tessera.personal.env.example)を参照してください。

**誰でも使える公開インスタンス。** 誰でもアップロードできるので、マシンのリソースはすべて見知らぬ人が使うことになります。ファイルサイズと総容量に上限を設け、既定の保存期間を短くし、登録は閉じるか招待制にして、マルウェアスキャナーにつないでください。[公開インスタンス向けの設定例](tessera.public.env.example)を参照してください。

**チームや組織で使う。** 匿名アップロードを無効にし、社内の OpenID Connect でサインインしてもらいます。ロールで、誰がアップロードできるか、誰が全員の共有を見られるか、誰が設定を変えられるかを決め、アカウントごとに容量の上限も設定できます。1 台で足りなくなったら PostgreSQL と S3 互換ストレージに移れます。

## 機能

### 内部の設計

- **大きなファイルはサーバーを経由しなくてよい。** S3 か Google Cloud Storage を使うと、アップロードはバケットへ直接送られ、通常のダウンロードもバケットへリダイレクトされます。パスワードやダウンロード回数の制限がある共有は必ずサーバーを経由するので、その制限は守られます。
- **負荷がかかっても崩れない制限。** 取得は条件付きのデータベース更新 1 回で数えるため、1 回限りの共有を 2 人が同時に取りに来ても、両方が手に入れることはありません。有効期限はリクエストごとに確認し、取得コードの照会にはレート制限があります。
- **バッファせずにストリーミング。** ファイルがどれだけ大きくても、メモリ使用量は一定のままです。
- **ストレージを組み合わせられる。** ローカルディスク、S3 互換ストレージ、Azure Blob、Google Cloud Storage、WebDAV を同時に使え、管理画面から再起動なしで追加・退役できます。大きなファイルを複数のバケットに分割して保存することもできます（初期状態では無効）。
- **SQLite・PostgreSQL・MySQL に対応**し、それぞれに手書きのマイグレーションがあります。
- **開発に使える API。** OpenAPI 仕様はサーバーのコードから生成され、CI で検証とファジングを行います。CLI と API トークンは Web 画面と同じエンドポイントを使います。
- **容量と制限**はロールごと、アカウントごと、匿名アップロード全体に設定できます。再開可能なアップロードと、任意のマルウェアスキャンにも対応しています。

### 使う人にとって

- **有効期限とダウンロード回数。** 共有ごとに保存期間を決め、取得回数の上限も設定できます。
- **パスワード。** 入力するまで、内容の表示もダウンロードもできません。
- **プレビュー。** コードはシンタックスハイライトされ、画像・SVG・Mermaid の図はページ内に表示されます。チャットに貼ったリンクにはプレビューカードが付きます。
- **フォルダーや複数のファイル**は、まとめて 1 つのアーカイブでダウンロードできます。
- **必要なときだけアカウント。** 登録は公開・招待制・停止から選べ、パスワード、GitHub、Discord、任意の OpenID Connect でサインインできます。
- **画面は英語・繁体字中国語・日本語に対応。**

## 管理画面

<p align="center">
  <img src="docs/images/screenshot-admin.png" alt="管理ダッシュボード" width="80%" />
</p>

運用に必要なものはすべて `/admin` にあり、ほとんどの設定は再起動なしで次のリクエストから反映されます。

- **ダッシュボード：** 使用容量とその保存先、転送量、推移、危険な設定の警告。
- **ユーザー：** アカウントの停止や削除、ロールの変更、アカウントごとの容量上限、あるアカウントがアップロードした共有の一括終了。
- **共有：** 匿名のものも含め、インスタンス上のすべての共有と、そのアップロード者。
- **招待コード、ロールと権限。**
- **設定：** サインイン方法、アップロード制限、既定の有効期限、ストレージバックエンド、メール、スキャン、リンクプレビュー、外観。

ブラウザがなくても、同じ設定はコマンドから変更できます：`tessera-server settings list`、`settings set <key> <value>`。

## クイックスタート

Docker で起動します。

```bash
docker run -d --name tessera --restart unless-stopped \
  -p 12923:12923 -v tessera-data:/data ghcr.io/limatura/tessera
```

<http://localhost:12923> を開いてください。初回起動時にサーバーが管理者アカウントを作成し、そのパスワードをログに一度だけ出力します。

```bash
docker logs tessera | grep password
```

このリポジトリをクローンした場所で `docker compose up -d` を実行しても同じで、設定は `docker-compose.yml` に残ります。

インターネットに公開する前に、[公開インスタンス向けの設定例](tessera.public.env.example)を読んでください。匿名アップロードは初期状態で有効なので、容量の上限がないと、サーバーに届く人なら誰でもディスクを埋められます。

### ソースから動かす

Rust と pnpm が必要です。

```bash
pnpm install
pnpm --filter web build
pnpm server:dev
```

## 設定

ほとんどの設定は管理画面にあります。ポート、データベースの URL、データディレクトリのように、サーバーの起動前に決まっている必要があるものだけは環境変数で指定します。

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=postgres://tessera:secret@localhost/tessera
TESSERA_SERVER__DATA_DIR=/var/lib/tessera
```

コメント付きの設定例が 2 つあります：[個人向け](tessera.personal.env.example)と[公開インスタンス向け](tessera.public.env.example)。systemd 用の [unit ファイル](contrib/tessera.service)もあります。

### ストレージ

ファイルはローカルのディレクトリに保存されます。ほかのバックエンドは管理画面から追加できます。

| バックエンド | 備考 |
| --- | --- |
| ローカルディスク | 初期設定 |
| S3 互換 | MinIO、Garage、RustFS、SeaweedFS、VersityGW、s3proxy でテスト済み。認証情報があれば AWS S3、Cloudflare R2、Backblaze B2 もテストします。 |
| Azure Blob Storage | Azurite でテスト済み |
| Google Cloud Storage | 組み込み済み、テストは未整備 |
| WebDAV | 小さなファイルのみ。上限は `share.inline_threshold_bytes`（既定 64 KiB） |

## コマンドラインクライアント

Linux または macOS に `tess` をインストールします。Windows 版は [リリースページ](https://github.com/Limatura/tessera/releases) にあり、`cargo install --path apps/cli` でソースからビルドすることもできます。

```bash
curl -fsSL https://raw.githubusercontent.com/Limatura/tessera/main/scripts/install.sh | sh
```

安定版のリリースまでは、`-s -- --version <バージョン>`を付けてください。

```bash
tess login https://share.example.com
tess send report.pdf --expires 24
tess get https://share.example.com/s/P8D72PSF
echo "hello" | tess send -
tess ls
tess rm P8D72PSF
tess
```

`tess` だけを実行するとフルスクリーン画面が開き、パスを入力せずにファイルを探して送信したり、自分の共有を管理したりできます。

<p align="center">
  <img src="docs/images/tess.gif" alt="tess でフォルダとファイルを選んで送信し、リンクと QR コードを受け取るところ" width="80%" />
</p>

`--encrypt` を付けると、内容はマシンを離れる前に暗号化されます。鍵はリンクの `#` 以降の部分で、サーバーが目にすることはありません。

CLI がなくても `curl` で送れます。リンクがそのまま表示されます：

```bash
curl -T report.pdf https://share.example.com/
echo "hello" | curl -T - https://share.example.com/
```

オプションはヘッダーで指定します：`-H 'X-Share-Expires: 1d'`、`-H 'X-Share-Max-Downloads: 3'`、`-H 'X-Share-Password: …'`。

## ドキュメント

- [利用・運用ガイド](https://github.com/Limatura/tessera-docs)
- [サーバーの内部](apps/server/README.md)と、[docs/](docs/README.md) にある設計メモ
- [API 仕様](openapi.json)（サーバーのコードから生成）

## コントリビュート

[CONTRIBUTING.md](CONTRIBUTING.md) を参照してください。セキュリティの問題は issue ではなく、[SECURITY.md](SECURITY.md) の手順で非公開で報告してください。

## ライセンス

[AGPL-3.0](LICENSE)。改変した Tessera を他の人に提供する場合は、そのソースコードを入手できるようにする必要があります。
