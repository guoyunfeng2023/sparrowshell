# SparrowShell Windows 开源客户端适配与构建指南

## 1. 文档目的

本文用于指导开发者在 **Windows 10／11 x64** 上，基于当前开源源码完成 Windows 客户端适配、开发运行、自动化测试和 NSIS 安装包构建。

技术架构与 Mac 版保持一致：

- Tauri 2；
- Rust stable；
- React 19 + TypeScript + Vite；
- WebView2，不内置 Chromium；
- SQLite 本地优先存储；
- SSH、SFTP、监控、AI、账号和同步继续复用现有 Rust 模块与后台协议。

**当前 Windows 工作副本已完成第 5 节所述平台适配，可直接按第 6～8 节运行、测试和构建。如果从仍以 macOS 为唯一目标的旧开源副本开始，则必须先完成第 5 节的平台适配，不能假设在 Windows 上执行一条构建命令就能直接成功。后续修改应保持 Mac 与 Windows 实现共存，不得用空实现替代业务功能。**

首版构建目标：

- Windows 10、Windows 11；
- x64：`x86_64-pc-windows-msvc`；
- NSIS `.exe` 安装包；
- 暂不支持 Windows ARM64、MSI 和便携版。

## 2. 源码目录

在仓库根目录操作，主要目录如下：

```text
sparrowshell/
└── apps/
    └── desktop/
        └── windows/            # Windows 完整客户端工程
            ├── src/            # React 前端源码
            ├── public/
            ├── src-tauri/      # Tauri 桌面入口
            │   ├── Cargo.toml
            │   ├── tauri.conf.json
            │   ├── icons/      # 已包含 icon.ico 等 Windows 图标
            │   └── src/
            ├── crates/         # SSH、SFTP、存储、AI、同步等 Rust 模块
            ├── Cargo.toml      # Windows 客户端 Rust workspace
            ├── Cargo.lock
            ├── package.json
            ├── package-lock.json
            ├── rust-toolchain.toml
            └── config.example.json  # 编译时直接内置的后台地址配置
```

不要从 Mac 电脑复制以下目录或文件到 Windows 参与构建：

```text
target/
node_modules/
dist/
.env
*.db
*.sqlite
```

这些构建产物、依赖缓存和本地数据必须在 Windows 上重新生成。不要把真实服务器配置、密码、私钥、登录令牌、支付私钥或许可证密钥带入开源工作区。

## 3. 准备 Windows 构建环境

建议使用一台真实的 Windows 11 x64 电脑，至少准备 16 GB 内存和充足的磁盘空间。Rust 首次编译可能产生较大的 `target` 目录。

### 3.1 必装软件

1. **Git**。
2. **Node.js 20 或更高版本**，同时安装 npm。
3. **Rustup、Rust stable 和 Cargo**，使用 MSVC 工具链。
4. **Visual Studio 2022 Build Tools**，安装：
   - “使用 C++ 的桌面开发”工作负载；
   - MSVC x64／x86 生成工具；
   - Windows 10 或 Windows 11 SDK。
5. **Microsoft WebView2 Runtime**。
6. **NASM 2.16 或更高版本**，并将 `nasm.exe` 所在目录加入 `PATH`；`russh` 依赖的 `aws-lc-rs` 在 Windows MSVC 下编译时需要它。
7. 可选：Visual Studio Code、Windows Terminal。

不需要单独全局安装 Tauri CLI，项目已经通过 `@tauri-apps/cli` 提供 `npm run tauri` 命令。

### 3.2 配置 Rust 目标

打开 PowerShell：

```powershell
rustup default stable-x86_64-pc-windows-msvc
rustup target add x86_64-pc-windows-msvc
```

检查环境：

```powershell
node --version
npm --version
rustc -vV
cargo --version
rustup target list --installed
```

确认 Rust 输出中的主机目标为：

```text
x86_64-pc-windows-msvc
```

如果提示找不到 `link.exe`，通常是 C++ Build Tools、MSVC 或 Windows SDK 没有安装完整，不要通过改用 GNU 工具链掩盖问题。

## 4. 获取源码并配置后台地址

### 4.1 安装前端依赖

在仓库根目录执行：

```powershell
cd apps/desktop/windows
npm ci
```

这里使用 `npm ci`，按照 `package-lock.json` 安装固定依赖，不要随意改成会更新锁文件的安装方式。

### 4.2 配置后台地址

直接编辑 **Windows 工程根目录**的 `apps/desktop/windows/config.example.json`：

```json
{
  "apiBaseUrl": "http://127.0.0.1:8070"
}
```

配置规则：

- 该文件会在编译时嵌入客户端，不是安装后的用户配置文件；**不需要复制或重命名为 `config.json`**。
- 修改后需要重新启动开发客户端或重新打包，开发模式和正式安装包使用同一个后台地址。
- 安装后不需要创建任何用户配置文件，客户端也不会读取用户目录中的 `config.json`。
- 需要更换后台时，修改 `config.example.json` 并重新构建客户端。
- 本机后台可以使用 `http://127.0.0.1:8070`。
- 非本机后台必须使用 HTTPS，例如：

```json
{
  "apiBaseUrl": "https://your-domain.example/sparrowshell"
}
```

- 配置 JSON 或地址格式错误时，构建或启动会明确报错。
- 不要在配置中写入模型密钥、服务器密码、登录令牌、支付私钥或其他秘密。
- 如果只构建客户端而不启动后台，本地 SSH、SFTP 等功能仍可验证；AI、账号、云同步、支付和反馈等依赖后台的功能不可用。

## 5. Windows 适配与维护要求

当前 Windows 工作副本已按本节完成平台适配。以下要求用于说明实现边界并指导后续维护：**使用 `#[cfg(target_os = "macos")]` 和 `#[cfg(target_os = "windows")]` 隔离实现，不要直接删除 Mac 实现，也不要为了通过编译把业务功能替换为空实现。**

### 5.1 隔离 macOS 专属 Rust 依赖

检查：

```text
apps/desktop/windows/src-tauri/Cargo.toml
apps/desktop/windows/src-tauri/src/lib.rs
```

当前存在的 Mac 专属内容包括：

- `objc2-app-kit`；
- `NSPasteboard`、`NSPasteboardTypeString`；
- Tauri 的 `macos-private-api` 能力。

处理要求：

- `objc2-app-kit` 只能作为 macOS 目标依赖。
- AppKit 的导入和调用必须只编译到 macOS。
- Tauri 的 Mac 私有 API 配置不能成为 Windows 构建障碍。
- Windows 使用自己的剪贴板实现，保持现有 `read_clipboard` 命令的前端调用签名不变。

可使用跨平台剪贴板库或 Tauri 剪贴板插件，但必须验证终端和远程文件编辑器中的实际粘贴行为。

### 5.2 用 Windows DPAPI 实现本机凭据存储

重点检查：

```text
crates/secrets/src/lib.rs
apps/desktop/windows/src-tauri/src/lib.rs
apps/desktop/windows/src-tauri/src/local_account_store.rs
```

当前 `KeychainStore` 只在 macOS 下提供，而桌面状态、SSH 会话和本地账号存储直接引用了它。

建议增加统一的平台类型，例如 `PlatformCredentialStore`：

- macOS：继续映射到现有 `KeychainStore`。
- Windows：实现 `DpapiCredentialStore`，使用 Windows DPAPI CurrentUser 加密本机秘密。

Windows 实现要求：

- 服务器密码、私钥口令等秘密使用 DPAPI 当前用户范围保护。
- 密文保存在应用私有数据目录，不同步云端。
- 实现现有 `CredentialStore` 的保存、读取和删除接口。
- 明文尽量使用 `Zeroizing`，避免无必要复制和日志输出。
- 更换 Windows 用户后不能读取原用户加密的数据，这是 DPAPI CurrentUser 的预期边界。
- 不得使用内存存储冒充持久化方案，也不得把密码明文写入 JSON、SQLite 或日志。

`LocalAccountStore` 当前还承担旧钥匙串项清理职责。Windows 版应通过平台抽象处理：没有旧钥匙串时不得因此编译失败或阻塞账号功能。

### 5.3 处理原生菜单和中英文切换

检查：

```text
apps/desktop/windows/src-tauri/src/native_menu.rs
apps/desktop/windows/src-tauri/src/lib.rs
```

macOS 的应用菜单包含“服务、隐藏、隐藏其他”等 Mac 交互。Windows 不应机械照搬。

处理要求：

- Mac 菜单安装逻辑只在 macOS 下执行。
- Windows 可以实现适合本平台的菜单，也可以由现有应用内入口承担设置和关于功能。
- 前端调用的 `set_app_locale` 命令必须保持兼容；如果 Windows 不使用原生菜单，应提供明确、安全的平台实现，不能因命令不存在导致语言切换报错。
- 不删除 Mac 的中英文菜单测试。

### 5.4 修正 Windows 更新目标

检查：

```text
crates/sync/src/http.rs
apps/desktop/windows/src/features/support/AboutPanel.tsx
apps/desktop/windows/src/features/support/LegalDialog.tsx
apps/desktop/windows/src/features/support/updateFlow.test.ts
```

当前客户端更新请求固定发送：

```text
darwin-aarch64
```

Windows x64 必须发送：

```text
windows-x86_64
```

建议定义编译期平台常量，再用于更新查询：

```rust
#[cfg(target_os = "macos")]
const UPDATE_TARGET: &str = "darwin-aarch64";

#[cfg(target_os = "windows")]
const UPDATE_TARGET: &str = "windows-x86_64";
```

同时处理：

- “关于”页面显示 `Windows · x64`。
- 隐私说明中的更新目标不再写成 `darwin-aarch64`。
- 保留后台原有字段和协议，不要为了 Windows 擅自修改后台接口结构。
- 测试必须覆盖 Mac 和 Windows 目标，防止两端互相抢更新包。

### 5.5 调整 Windows 配置和安装包图标

检查：

```text
apps/desktop/windows/src-tauri/tauri.conf.json
apps/desktop/windows/src-tauri/icons/
```

要求：

- 保留应用标识 `com.sparrowshell.desktop`，避免无必要改变应用数据归属。
- `bundle.icon` 应包含现有的 `icons/icon.ico`，确保 Windows 安装程序和任务栏图标正确。
- Mac 的 `macOSPrivateApi` 和最低系统版本配置应与 Windows 构建隔离，可使用平台配置或条件构建方式。
- 首版通过命令行只构建 NSIS，不额外生成 MSI。
- 不修改 `devUrl` 的端口约定；当前 Vite 使用 `127.0.0.1:4179` 且 `strictPort=true`。

### 5.6 修正界面中的平台文案和快捷键

全局搜索：

```text
macOS
Keychain
钥匙串
darwin-aarch64
Command
⌘
```

重点处理：

- Windows 文案改为“本机 Windows”和“Windows DPAPI”等真实实现，不再宣传 macOS 钥匙串。
- Mac 的 `Command+C/V/A` 对应 Windows 的 `Ctrl+C/V/A`。
- 搜索、保存等快捷键提示在 Windows 显示 `Ctrl+F`、`Ctrl+S`。
- 不机械迁移 `Command+H` 等 Mac 专属能力。
- 界面预览、账号、关于等页面不再提示“请使用 macOS 客户端”。
- 不只做文字替换；必须同时验证对应快捷键和存储实现。

### 5.7 验证 Windows 路径和本地数据安全

本地数据继续通过 Tauri 的 `app_data_dir()` 获取，不要把路径写死为 macOS 目录。

必须验证：

- 中文用户名和中文路径下可以创建、读取数据库。
- 覆盖安装和升级不会删除 SQLite、服务器配置和本机秘密。
- 开发版与正式版数据继续隔离。
- Windows 资源管理器拖入一个或多个普通文件可以上传；中文文件名和空格路径可用。
- 私钥路径选择、下载保存路径和远程文件编辑保存均可用。
- 卸载默认保留用户数据，除非产品另有明确提示。

## 6. 开发运行

完成平台适配后，在 `apps/desktop/windows` 下执行：

```powershell
npm run build
npm run tauri dev
```

说明：

- `npm run build` 只验证 TypeScript 和前端生产构建。
- `npm run tauri dev` 会启动 Vite 和 Rust 桌面应用。
- 如果 `4179` 端口被占用，先关闭旧开发进程，不要忽略 `strictPort` 报错。
- 首次 Rust 编译耗时较长，后续会使用增量缓存。

## 7. 自动化测试

前端测试：

```powershell
cd apps/desktop/windows
npm test
```

继续在 `apps/desktop/windows` 工程根目录运行 Rust 工作区测试：

```powershell
cargo test --workspace
```

要求：

- 不通过删除或忽略失败测试来换取构建成功。
- 为 Windows DPAPI、更新目标、快捷键和平台文案补充测试。
- 保留 Mac 专属测试的 `cfg`，确保 Windows 不会执行 macOS 钥匙串集成测试。
- OpenSSH Docker 集成测试不是首次构建的前置条件，可在核心构建完成后单独执行。

## 8. 构建 Windows NSIS 安装包

在 `apps/desktop/windows` 下执行：

```powershell
npm run tauri build -- --bundles nsis --target x86_64-pc-windows-msvc
```

由于 Windows 工程使用 Rust workspace，未自定义目标目录时，安装包通常位于 **Windows 工程根目录**：

```text
apps/desktop/windows/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe
```

如果设置了 `CARGO_TARGET_DIR`，则以该环境变量指定的目录为准。不要机械地在 `apps/desktop/windows/src-tauri/target` 下寻找产物。

在 `apps/desktop/windows` 下计算安装包 SHA-256：

```powershell
Get-FileHash `
  -Path "target/x86_64-pc-windows-msvc/release/bundle/nsis/实际安装包名称.exe" `
  -Algorithm SHA256
```

## 9. 签名与发布边界

- 未签名安装包只能用于开发和内部测试。
- 正式公开发布前，使用 Authenticode 代码签名证书对客户端可执行文件和安装包签名。
- 签名不能替代 SmartScreen 信誉积累，仍应在干净 Windows 环境验证安装提示。
- 不将证书私钥、证书密码或任何生产密钥提交到仓库。
- 发布版本时保持 `apps/desktop/windows/package.json`、`src-tauri/Cargo.toml` 和 `tauri.conf.json` 中的版本号一致。
- 发布记录使用 Windows 目标 `windows-x86_64`，不要上传到 Mac 的 `darwin-aarch64` 版本记录。
- 安装包和哈希验证通过后，再进行覆盖安装和数据保留测试。

## 10. 最低验收清单

### 构建与安装

- [ ] `npm run build` 成功。
- [ ] `npm test` 通过。
- [ ] `cargo test --workspace` 通过。
- [ ] `npm run tauri dev` 可以启动真实桌面客户端。
- [ ] NSIS `.exe` 构建成功，并记录文件大小和 SHA-256。
- [ ] Windows 10 或干净 Windows 11 环境可以安装和启动。

### 核心功能

- [ ] 新建文件夹、子文件夹和服务器配置。
- [ ] 密码、私钥和跳板机连接 Linux 测试服务器。
- [ ] 终端输入、复制、粘贴、历史命令和命令提示正常。
- [ ] SFTP 目录浏览、上传、下载、编辑和解压正常。
- [ ] CPU、内存、磁盘和进程监控正常。
- [ ] 重启客户端后，本机服务器配置和受保护秘密可以恢复。
- [ ] 浅色／深色主题、中英文和 Ctrl 快捷键正常。
- [ ] 中文路径、中文文件名及 125%／150% DPI 可用。

### 后台相关功能

- [ ] 配置的后台地址与预期环境一致。
- [ ] AI、账号、云同步和反馈连接的是测试后台。
- [ ] 更新请求目标为 `windows-x86_64`。
- [ ] 不会向 Windows 客户端返回 Mac 安装包。
- [ ] 日志、前端源码和安装包中不包含真实密码、私钥、令牌或支付秘密。

## 11. 常见问题

### `objc2-app-kit` 或 `NSPasteboard` 编译失败

说明 AppKit 依赖或剪贴板调用没有按 macOS 目标隔离，返回第 5.1 节处理。

### `KeychainStore` 找不到

说明 Windows 仍在直接使用 Mac 钥匙串类型。应引入平台凭据存储抽象和 DPAPI 实现，不要删除 SSH 密码保存功能。

### 找不到 `link.exe`

安装或修复 Visual Studio 2022 C++ Build Tools、MSVC x64 生成工具和 Windows SDK，然后重新打开 PowerShell。

### 提示找不到 `nasm` 或 `aws-lc-rs` 编译失败

安装 NASM 2.16 或更高版本，把 `nasm.exe` 所在目录加入 `PATH`，重新打开 PowerShell 后再构建。不要通过删除 SSH 依赖绕过该问题。

### 安装后窗口无法显示

检查 WebView2 Runtime 是否安装，并查看应用日志。不要通过内置 Chromium 改变项目架构。

### 更新检查返回没有适用版本

检查客户端是否仍发送 `darwin-aarch64`，以及后台版本记录是否使用 `windows-x86_64`。

### 构建成功但找不到安装包

Windows 工程是独立的 Rust workspace，优先检查：

```text
apps/desktop/windows/target/x86_64-pc-windows-msvc/release/bundle/nsis/
```

同时检查是否设置了 `CARGO_TARGET_DIR`。

### 未签名安装包被 SmartScreen 或杀毒软件提示

开发阶段只在可信测试环境验证。正式发布使用 Authenticode 签名，不要把绕过安全提示作为发布方案。

## 12. 可直接交给 Windows 开发助手的执行要求

```text
请阅读 apps/desktop/windows/docs/Windows开源客户端适配与构建指南.md，
并在 apps/desktop/windows 工程根目录中执行。

目标平台为 Windows 10/11 x64，技术栈保持 Tauri 2 + Rust + React + TypeScript，
最终生成 NSIS 安装包，目标三元组为 x86_64-pc-windows-msvc。

当前源码已经完成 Windows 平台适配。后续维护必须保留跨平台条件编译，
不要删除 Mac 实现，也不要为通过编译而删除 SSH、凭据保存、菜单、更新或其他业务功能。

必须保持：
1. AppKit 剪贴板和 macOS 专属 Cargo 依赖的条件编译。
2. KeychainStore 的平台抽象，Windows 使用 DPAPI CurrentUser 持久化保护秘密。
3. Windows 菜单与 set_app_locale 命令兼容。
4. Windows 更新目标为 windows-x86_64，并保留 Mac 的 darwin-aarch64。
5. Windows 图标、Ctrl 快捷键、平台文案、本地路径和中文文件名支持。
6. 直接修改并内置 config.example.json，不需要 config.json；配置中不写入任何密钥。

修改后依次运行 npm run build、npm test、cargo test --workspace、npm run tauri dev，
最后执行 npm run tauri build -- --bundles nsis --target x86_64-pc-windows-msvc。

请报告：修改文件、测试结果、开发运行结果、安装包绝对路径、文件大小、SHA-256、
尚未解决的问题。不要声称未实际执行的测试已经通过，不要上传任何真实用户数据或生产秘密。
```

---

本指南依据当前开源副本的实际代码和配置整理。Windows 工程已执行前端构建、前端测试和 Rust 编译检查；安装包验收以实际构建日志和测试结果为准。
