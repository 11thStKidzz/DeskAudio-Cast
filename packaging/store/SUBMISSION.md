# Microsoft Store 提交说明（发布者操作资料，2026-10-22）

不自动上传、提交、接受协议或选择价格。软件 0.1.11 基于用户已实测通过的 0.1.9；MSIX 1.0.11.0 为独立技术包版本，不是正式发布承诺。

## 身份与名称

- Package/Identity/Name：`11thStKidzz.DeskAudioCast`
- Package/Identity/Publisher：`CN=F8685FFB-73E3-4962-83BD-CEE132D1ADF6`
- Package/Properties/PublisherDisplayName：`11thStKidzz`
- 简体中文显示名：桌面声渡；默认英文显示名：DeskAudio Cast。
- 支持/隐私邮箱：woyaotingspotify@gmail.com。无需虚构官网或公开个人地址。若添加中文商店条目时要求名称预留，请在同一产品下确认“桌面声渡”也可用；不能假定英文预留自动覆盖中文名称。

## 可复制的商店描述：简体中文

将 Windows 系统声音无线投送到同一可信局域网中的兼容 HomePod。
需要 Windows 10 build 19041 或更新的 x64 系统；完整包内置固定版 WebView2，无需单独安装。MSIX 使用商店部署的 VCLibs Desktop 框架依赖。

桌面声渡提供设备发现、单台音箱连接、音量控制，以及极低延迟、均衡、稳定三种模式。支持托盘控制、记住偏好、可选开机启动和联网自动投送。系统回环采集不需要额外虚拟声卡或驱动。

采用 ALAC 编码，在当前 44.1 kHz、16 位立体声传输格式下保留无损编码；不承诺任意源格式全链路位精确。延迟受网络和接收设备影响，不承诺零延迟或游戏级同步。需要兼容的实际音箱；本版本不以屏幕投射、多房间或立体声配对为卖点。

这是独立修改的开源应用，不是 Apple 官方或认证产品。HomePod 和 AirPlay 仅用于兼容性说明。按 GPLv3 或更新版本提供，第三方保留各自许可；付费分发不限制适用开源许可证授予的修改、复制和再分发权利。完整对应源码：https://github.com/11thStKidzz/DeskAudio-Cast/releases/tag/v0.1.11 。

建议功能条目：系统声音无线投送；局域网设备发现；三档播放模式；托盘音量及停止/继续；可选自动投送和开机启动；本地诊断导出；中英西俄德日界面。只宣称实际已实现功能，复杂网络/设备组合不保证兼容。

## Store description: English

Send Windows system audio wirelessly to a compatible HomePod on your trusted local network.
Requires Windows 10 build 19041 or later Windows 10/11, x64. Complete packages include Fixed Version WebView2; no separate WebView2 installation is required. MSIX uses the Store-deployed VCLibs Desktop framework.

DeskAudio Cast includes device discovery, a single-speaker connection, volume control, and Ultra-low, Balanced and Stable playback modes. Tray controls, saved preferences, optional startup and automatic sending are included. Windows loopback capture requires no extra virtual audio cable or driver.

ALAC encoding is lossless for the application's 44.1 kHz, 16-bit stereo transport format; arbitrary source formats are not claimed to be bit-perfect throughout the pipeline. Latency and compatibility depend on your network and receiver. Zero latency and gaming-grade synchronization are not guaranteed. A compatible physical speaker is required. Screen casting, multi-room and stereo-pair operation are not advertised features.

An independent modified open-source application, not an official or Apple-certified product. HomePod and AirPlay names describe compatibility only. Applicable code is GPLv3-or-later, with third-party licenses retained. A paid download does not remove recipients' open-source modification, copying and redistribution rights. Complete corresponding source: https://github.com/11thStKidzz/DeskAudio-Cast/releases/tag/v0.1.11 .

Features: system audio casting; LAN discovery; three playback modes; tray volume and stop/resume; optional automatic sending and startup; local diagnostics; Chinese, English, Spanish, Russian, German and Japanese interfaces.

## 系统要求 / System requirements

Windows 10 2004/build 19041 或更新的 Windows 10/11，x64；兼容 HomePod、可信可互通网络和 Windows 声音输出。完整包内置固定版 WebView2；缺失时提示重新安装完整包及诊断位置。MSIX 声明 Microsoft.VCLibs.140.00.UWPDesktop≥14.0.33728.0，由商店部署；NSIS 附微软 VC++ x64 离线安装器，仅在同意后运行微软界面并复查版本。固定运行时安全补丁须随应用更新。干净 Windows 实测仍是发布验收项，不把结构检查当成开箱即用验收。固定运行时微软再分发条款尚待发布者确认，此份本地候选不得据此宣布可提交。

## 审核说明 / Notes for certification

This is a packaged Win32 desktop audio sender. It does not require an app account, subscription or login. Testing audio requires a real compatible HomePod on a reachable trusted LAN. Start the app, select the discovered receiver, connect, start sending, then play audio on Windows. Check volume, stop/resume and closing to tray. Quit from the tray to fully exit. Do not disable the firewall; use only appropriate local network rules if prompted. VPN routing or receiver access settings may prevent discovery/connection. No microphone recording or cloud audio upload is implemented.

`runFullTrust` is required for the existing Win32 process, WASAPI output-loopback capture and system tray. LAN client/server capability is used for mDNS discovery and audio/control traffic. StartupTask is opt-in (`Enabled=false`); disabling it in Windows is respected. Internet capability does not introduce a telemetry endpoint. Do not claim these capabilities are a Microsoft security certification.

No trial, purchase API or app-level DRM is implemented. Price, trial settings, sales territories and tax/account matters must be chosen by the publisher in Partner Center; no choices have been made on their behalf. Charging for GPL software is not itself proof of Store acceptance or clearance of third-party rights.

## 隐私、许可和对应源码

- 隐私全文：`legal/PRIVACY.txt`。应用内已有固定入口。官方 MSIX 支持资料说明可粘贴政策文本，不必自建官网；同时 Store 政策 10.5.1 要求政策 URL。以实际 Partner Center 提交页面和适用政策为准：若没有可用的托管文本入口，需另获授权把政策托管成公开 HTTPS 页面，不可填假地址。
- 支持字段可以直接填上述邮箱；官网字段可留空。不把邮箱冒充网页 URL。
- 许可草案：`legal/STORE_EULA_DRAFT.txt`，完整 GPL：根 `LICENSE`，第三方资料：`legal/third-party`、`legal/native`。应在商店明确提供适用 GPL 及第三方条款，不照搬禁止修改/转售的标准软件许可；账户、支付和商店服务条款不由本应用改写。
- 已查阅公开 ADA v8.10 §3(c)：开源合规及源码提供仍是发布者责任；其标准应用许可说明随附不同条款时采用不同条款。这为配置开源许可提供依据，不等于已审阅发布者账户内最终合同或保证 GPL 与所有商店服务条款无冲突。不得替用户接受协议。
- 对应源码 ZIP 必须和二进制同一指纹，含依赖及构建资料并经独立构建验证。用户指定仓库为 https://github.com/11thStKidzz/DeskAudio-Cast ，同版本 Release 提供完整 ZIP。上传后需匿名下载核对真实 SHA；仅给上游仓库、邮件支持或空仓库不算已履行源码访问义务。
- 专利、商标、产品名称可用性及目标市场的法定义务保留为外部确认项。本轮不重做 F1～F5 授权历史，不把“未发现”写成“取得授权”。

## 提交前仍需完成

1. 获准公开对应源码，并核实实际可下载；配置可访问的隐私资料。
2. 真实的商店截图、年龄评级问卷、价格/市场及账户内协议确认。不要填写私人信息到项目，账户所需身份/税务只在官方页面操作。
3. 当前 MSIX 是未签名上传候选，不能靠双击证明可安装。WACK 工具虽在本机，可信安装/签名和认证测试尚未完成；可在获准后选择测试证书或商店私有测试分发，不能擅自信任证书、上传或正式发布。
4. 对签名 MSIX 验证首次启动、系统声音/网络、托盘、数据重定向、StartupTask、升级/卸载；干净 Windows 的 WebView2/VC++ 依赖部署也必须验收。0.1.9 NSIS 的用户验收不替代这些测试。

## 官方依据（2026-10-22 核查）

- [包身份、版本及签名要求](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- [隐私文本与支持邮箱](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/support-info)
- [Store 政策 v7.20](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies)
- [公开 ADA v8.10](https://cdn-dynmedia-1.microsoft.com/is/content/microsoftcorp/microsoft/store/documents/legal/ada/fy26/MS.Store.ADAv8.10.EN.US.pdf)
- [MakePri](https://learn.microsoft.com/en-us/windows/uwp/app-resources/compile-resources-manually-with-makepri)
