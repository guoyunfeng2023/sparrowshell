# Docker 部署示例

本目录提供后台服务和管理端的镜像构建示例。它们不包含任何真实环境配置，也不会自动创建或升级数据库。

## 组件

- `server/Dockerfile`：运行 Java 8 后台服务。
- `admin/Dockerfile`：使用 Nginx 托管管理端静态文件。
- `admin/nginx.conf`：将同源 `/v1/` 请求转发到后台容器。
- `server/sparrowshell.env.example`：完整平台的环境变量模板。

## 1. 准备 MySQL 5.7

在仓库根目录执行完整初始化脚本：

```bash
mysql -u root -p < deploy/mysql/init.sql
```

脚本会创建 `sparrowshell` 数据库和全部数据表。也可以在 MySQL 客户端中执行：

```sql
SOURCE /绝对路径/deploy/mysql/init.sql;
```

`init.sql` **仅用于全新部署，不要在已有业务数据库上重复执行**。请为应用另外创建最小权限账号，不要使用生产 root 账号作为应用账号。应用启动时不会自动执行初始化 SQL。

仅运行 AI 基础服务时，可以不配置 MySQL，并保持以下平台能力关闭：

```dotenv
SPARROWSHELL_PLATFORM_PERSISTENCE_ENABLED=false
SPARROWSHELL_PLATFORM_WORKER_ENABLED=false
SPARROWSHELL_USAGE_ENABLED=false
SPARROWSHELL_BILLING_ENABLED=false
SPARROWSHELL_MAIL_ENABLED=false
```

此模式不提供账号、云同步、反馈持久化、版本发布、订单和管理端数据能力。

## 2. 构建后台镜像

在仓库根目录执行：

```bash
cd services/sparrowshell-ai
mvn clean package
cp target/sparrowshell-ai-0.1.0.jar \
  ../../deploy/docker/server/sparrowshell-server-0.1.0.jar
cd ../..
docker build -t sparrowshell-server:local deploy/docker/server
```

## 3. 构建管理端镜像

```bash
cd apps/admin
npm ci
npm run build
cp -R dist ../../deploy/docker/admin/
cd ../..
docker build -t sparrowshell-admin:local deploy/docker/admin
```

管理端默认请求同源 `/v1/admin`，Nginx 会保留 `/v1/` 前缀并转发给后台。因此，使用本目录的镜像时不需要写入后台域名。

## 4. 准备本地配置

```bash
cp deploy/docker/server/sparrowshell.env.example \
  deploy/docker/server/sparrowshell.env
cp services/sparrowshell-ai/config/application.properties.example \
  services/sparrowshell-ai/config/application.properties
```

编辑 `sparrowshell.env`，替换数据库等平台配置的占位符。这个文件已被 `.gitignore` 排除，不要提交实际值。

通义千问不再使用环境变量。编辑 `services/sparrowshell-ai/config/application.properties`：

```properties
sparrowshell.qwen.api-key=你的百炼API-Key
sparrowshell.qwen.app-id=你的百炼应用ID
sparrowshell.qwen.model=qwen3.8-flash
```

真实的 `application.properties` 已被 Git 忽略。启动容器时，将它以只读方式挂载到 `/app/config/application.properties` 即可，不需要重新构建镜像。

只需要 AI 基础功能时，参考上一节关闭数据库相关平台能力。需要账号、同步、更新、反馈或管理端时再启用完整平台能力。

## 5. 启动容器

```bash
docker network create sparrowshell-net

docker run -d \
  --name sparrowshell-server \
  --network sparrowshell-net \
  --restart unless-stopped \
  --env-file deploy/docker/server/sparrowshell.env \
  -p 127.0.0.1:8070:8070 \
  -v "$PWD/services/sparrowshell-ai/config/application.properties:/app/config/application.properties:ro" \
  -v sparrowshell-feedback:/data/feedback \
  sparrowshell-server:local

docker run -d \
  --name sparrowshell-admin \
  --network sparrowshell-net \
  --restart unless-stopped \
  -p 127.0.0.1:8080:80 \
  sparrowshell-admin:local
```

本机管理端地址为 `http://127.0.0.1:8080`。可通过容器状态和日志检查后台是否正常启动：

```bash
docker ps --filter name=sparrowshell
docker logs sparrowshell-server
```

`host.docker.internal` 在 Docker Desktop 可直接使用。Linux 宿主机需要自行改成容器可访问的数据库地址，或配置对应的 `host-gateway`；不要把数据库密码写入 Dockerfile。

## 6. 首次创建管理员

只有启用完整平台能力后才需要管理端账号。临时在环境文件中设置：

```dotenv
SPARROWSHELL_ADMIN_BOOTSTRAP_ENABLED=true
SPARROWSHELL_ADMIN_BOOTSTRAP_EMAIL=admin@example.com
SPARROWSHELL_ADMIN_BOOTSTRAP_PASSWORD=替换为至少12位的强密码
SPARROWSHELL_ADMIN_BOOTSTRAP_MFA_SECRET=替换为Base32格式的MFA密钥
```

重建后台容器并确认管理员创建成功后，立即将 `SPARROWSHELL_ADMIN_BOOTSTRAP_ENABLED` 改回 `false`，再次重建容器。不要把真实管理员密码或 MFA 密钥提交到仓库。

## 7. 客户端连接

修改 Mac 客户端根目录的 `apps/desktop/mac/config.example.json`（Windows 客户端使用 `apps/desktop/windows/config.example.json`）：

```json
{
  "apiBaseUrl": "http://127.0.0.1:8070"
}
```

这份配置会在编译时嵌入客户端，开发和打包使用同一个地址。安装后不需要创建用户配置文件；更换后台时需要修改配置并重新打包。非本机地址必须使用 HTTPS。

## 生产环境提醒

- 示例端口只绑定 `127.0.0.1`，公网部署请通过启用 TLS 的反向代理访问。
- 管理端不应无保护地暴露到公网；建议增加访问控制、IP 白名单或 VPN。
- 支付回调必须能通过公网 HTTPS 访问，并配置正确的 `SPARROWSHELL_PUBLIC_BASE_URL`。
- `SPARROWSHELL_BILLING_TEST_AMOUNT` 正式环境必须留空，否则可能覆盖真实套餐金额。
- 不需要支付时保持 `SPARROWSHELL_PAYMENT_CHANNEL=disabled`。
- 定期备份 MySQL 和反馈附件卷，妥善轮换模型、数据库、邮箱、管理员、许可证及支付密钥。
