# 桌面声渡 0.1.10：对应源码构建 / Corresponding-source build

目标：Windows x64，Windows 10 2004（19041）或更新版本。

完整源码下载：https://github.com/HotRiver7/DeskAudio-Cast/releases/tag/v0.1.10 。选择 DeskAudioCast-0.1.10-corresponding-source.zip；GitHub 自动生成的源码包不包含锁定依赖快照。

## 内容

- 应用、WASAPI 采集、UI、GPLv3 协议适配、锁文件、构建/测试脚本及法律资料。
- third-party/rust：由 Cargo 官方 vendoring 生成的锁定注册表依赖，保留校验文件、各包原始源码与许可声明。
- ui/node_modules：本机已有、与锁文件版本对应的 Windows 构建依赖副本。不是新安装，也未把整个用户 npm 缓存打包。快照有逐文件校验，但未重新从注册表逐个下载并复验 npm tarball。
- 只包含当前 GPLv3 协议目录，不包含旧 GPLv2 分支、Git 历史、日志、身份文件、旧 EXE/PDB 或个人设置。协议库未使用的上游 Cargo.lock 不作为产品输入；根 Cargo.lock 是应用的依赖基线。
- 原生 WebView2 loader 的上游二进制随其原包保留；已与 Microsoft 同版本 NuGet 包逐文件比对并保留原始 BSD-3-Clause 文本，见 legal/native。另行安装的 WebView2 Runtime 和 VC++ 系统运行库不捆绑在本包中。

## 本机需要的已有工具

官方 Rust MSVC 工具链、Node.js/npm，以及 Microsoft Visual Studio C++ Build Tools 和 Windows SDK。具体本次版本记录在构建记录中。缺工具时先征求使用者同意，不自动下载安装。

解压到新目录，在 PowerShell 中运行：

    .\tools\build_source_package.ps1

脚本校验 SOURCE_PACKAGE_MANIFEST.json 和源码指纹，使用新的空 Cargo 缓存与新的构建目录，执行界面测试/构建、Python 测试、Rust 工作区测试和 Release 构建；Cargo 始终使用 --locked --offline，不下载依赖，不上传文件。历史候选来源比较测试需私人历史副本，交付包不含该副本，明确跳过；不以跳过宣称全部测试通过。

本地打包：使用 tools/package_windows.ps1 -ReleaseDirectory <已验证的Release目录> -Makensis <NSIS编译器路径>。需要已有官方 NSIS 3.11 便携工具及 Windows SDK MakeAppx、MakePri；不会自动下载安装。NSIS 原始官方 ZIP SHA-256 为 c7d27f780ddb6cffb4730138cd1591e841f4b7edb155856901cdf5f214394fa1，下载地址见 legal/native/COMPONENTS.json。打包脚本、原始许可、图标、安装/卸载列表生成规则都在源码中。

应用版本 0.1.10；MSIX 独立包版本 1.0.10.0（Store 要求首段不为 0、第四段为 0）。公开身份为 11thStKidzz.DeskAudioCast，Publisher 为 CN=F8685FFB-73E3-4962-83BD-CEE132D1ADF6，发布者显示名 11thStKidzz。这些是公开包元数据，不是签名私钥。packaging/windows/Strings 经 MakePri 编译：en-US 默认 DeskAudio Cast，简体中文桌面声渡。工具核对精确身份、PRI 内容与 ASCII 文件名，输出未签名 store-upload.msix 候选。它不会上传、提交或签名；本机不能直接双击可信安装，Store 审核发布后由 Microsoft 签名。不擅自添加本地受信任证书。提交准备见 packaging/store/SUBMISSION.md。

构建结果在新解压目录的 target/archive-check/release。输出需要 WebView2 Runtime；MSIX 的 VC++ 桌面框架由声明依赖提供，独立 NSIS 使用系统 VC++ x64 运行库。源码构建不等于安装或商店认证。

完整上游音频库的额外独立单元测试与产品工作区测试分开：本轮在隔离副本中执行，其测试专用依赖/锁定过程不改变产品 Cargo.lock。源码包的离线构建验收针对最终应用，而非全部可选上游 benchmark/测试目标。

## 对应关系

SOURCE_PACKAGE_MANIFEST.json 记录各文件哈希和产品源码指纹。构建程序内嵌相同产品指纹。编译器、链接器时间戳及环境可能影响二进制字节，因此没有承诺逐字节可复现。

保留第三方原始许可与版权；收费不限制 GPL 接收者的修改、复制及再分发权利。网络分发时，按 GPLv3 §6 提供完整对应源码访问，保留每个分发版本的归档。
