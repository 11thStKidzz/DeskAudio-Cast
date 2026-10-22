# DeskAudio Cast 0.1.11：Windows 部署 / Windows deployment

## 固定运行时 / Fixed Runtime

应用包布局为 `DeskAudioCast.exe`、`WebView2Loader.dll`、`WebView2Fixed/msedgewebview2.exe` 及完整微软原始目录、法律资料和资源。MSIX 和 NSIS 使用同一布局，不能仅分发 EXE。当前锁定版本/官方来源/哈希在 packaging/windows/runtime-lock.json，原始256文件清单在 webview2-files.sha256.json。

实际编译发现 Tauri 的 fixedRuntime 打包配置要求编译时有整个运行时资源，不利于独立GPL源码构建。因此本项目对Tauri内置安装流程设置skip（不是使用系统Evergreen），由 runtime_prerequisites 在创建任何WebView/工作线程前从 current_exe 解析相邻 WebView2Fixed，检查关键文件，再设置微软官方 WEBVIEW2_BROWSER_EXECUTABLE_FOLDER。Wry0.55.1调用 WebView2环境创建API，微软Loader使用该绝对路径。我们的 tools/package_windows.ps1 分别为自定义MSIX/NSIS复制全部文件；缺失/不完整时提示并停止，不回退到已安装Evergreen。编译源码不再需要700MB专有运行时，实际启动和打包才需要。

不向系统安装 Fixed Runtime、不写其注册表、不创建更新服务。固定版本不会自动更新，发布者须跟进微软安全修复、更新 runtime-lock/文件清单并重新构建、验收。解压体积约705MB；包体积以实际产物记录为准。Windows10未打包Win32需要应用容器RX权限，NSIS仅按微软文档对自己的WebView2Fixed子树配置；MSIX由包部署权限处理，不修改系统ACL。Windows11不需要上述额外ACL步骤。UNC网络路径不受微软Fixed Runtime支持。

Fixed Runtime layout is shared by the custom MSIX and NSIS. Tauri resolves it relative to the executable, not the working directory. No Evergreen registration or separate WebView2 installation is needed. Security updates require distributing a newer app/runtime package. Windows10 unpackaged installations require Microsoft's documented renderer AppContainer read/execute ACLs; these are confined to the app's Runtime subtree. No security features are disabled.

## 官方材料获取 / Official materials

开发者直接从微软取得并核对条款后的固定运行时CAB，可用 `pwsh -File tools/fetch_windows_runtimes.ps1 -CabPath <本地CAB>` 导入；本项目不提供固定运行时二进制下载服务或其直接下载快捷方式。工具验证锁定SHA-256、核心微软签名和全部256个运行时文件，使用Windows expand解压，不执行下载程序。不加参数只检查已有缓存；-Download只允许取得锁定的官方VC++安装器。哈希是在本次官方HTTPS下载后固定的，不是声称微软另行公布了CAB校验值。任何下载哈希不同都必须调查，不能跳过验证。

The fetcher verifies pinned official materials; it does not install or execute them. Runtime binaries and VC redistributables belong in ignored local-tools/local-packages only, never Git. The proprietary Runtime is a separately licensed component, not relicensed as GPL. Preserve its original files and embedded third-party credits. See legal/native/COMPONENTS.json for license evidence and any remaining review limitations.

本轮用户已明确：先完成本地技术候选，正式固定运行时分发的法律确认留到发布前。微软原始EULA已保留；§2(b)用户条款/赔偿责任、§2(c)限制、§8通知与应用GPL/运行时分别授权等仍须发布者确认。未替用户接受再分发合同，未因代码或包构建成功宣布可以公开发布运行时。

## VC++ 与失败处理 / VC++ and failures

实际应用EXE导入 VCRUNTIME140 和 VCRUNTIME140_1，生成的非必需库DLL导入 VCRUNTIME140；UCRT由目标Windows提供。微软Loader及固定运行时的依赖另作实际包检查。MSIX保留 Microsoft.VCLibs.140.00.UWPDesktop >=14.0.33728.0，让商店部署；不能把它当作WebView2安装器。

NSIS先检查x64两份DLL及版本（最低14.51.36231.0，与当前构建工具匹配）。缺失时默认拒绝，说明管理员提示及微软许可后，用户可选择运行附带微软官方离线安装器；其许可由用户在微软界面确认。检查真实退出码并再次查DLL版本，非零失败、取消或版本仍不足均停止；3010提示重启但不自动重启；1638也必须通过版本复查。静默安装缺依赖时直接失败，不暗中提升权限。

运行时缺失/初始化失败由原生消息框提示修复方法；单个覆盖写入的 diagnostics/deployment-error.txt 只含固定错误代码、应用及期望运行时版本，不含原始OS异常、音频或用户路径。VC安装器的详细日志由微软写入临时目录；不将其自动上传或并入音频诊断。

## 验收限制 / Validation limits

存在于目录不等于应用实际使用。候选验收须检查实际子进程路径/版本、缺少Evergreen的环境、真实中英文法律弹窗、安装取消/失败与安装成功后的行为。MSIX本地签名/信任测试须另获同意，不把未签名包直接安装失败当成运行时缺陷。没有实际执行的测试必须标为未执行；源码和包结构检查不等于全新Windows或Store认证。

官方依据：

- https://v2.tauri.app/distribute/windows-installer/#fixed-version
- https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
- https://learn.microsoft.com/en-us/microsoft-edge/webview2/samples/webview2-winui3-sample
- https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist
