# 桌面声渡 / DeskAudio Cast

Windows 系统音频投送工具，版本 0.1.13（MSIX 1.0.13.0），x64。
使用 WASAPI 回环捕获所选输出的声音，以 ALAC 编码发送到可信网络中的单台兼容接收设备。无需虚拟声卡。
支持设备发现、音量、三档模式、托盘控制、可选开机启动与自动投送、六种界面语言和本地诊断。
延迟与兼容性受网络、设备和系统影响；不承诺零延迟或游戏级同步。不提供屏幕投射、多房间或立体声配对功能。

A Windows system-audio sender, version 0.1.13 (MSIX 1.0.13.0), x64.
WASAPI output loopback and ALAC transport to a single compatible receiver on a trusted network, without a virtual audio cable.
Includes discovery, volume, three modes, tray controls, optional startup/automatic sending, six UI languages and local diagnostics.
Latency and compatibility vary; zero latency and gaming-grade synchronization are not guaranteed. Screen casting, multi-room and stereo pairs are not offered.

## 安装 / Installation

Windows 10 2004/build 19041 或更新的 Windows 10/11，x64，需要兼容 HomePod 和 Microsoft WebView2 Runtime。
MSIX 声明 Microsoft VCLibs Desktop 框架依赖，由 Microsoft Store 部署；独立 NSIS 安装需要系统 VC++ x64 运行库。
缺少 WebView2 时，程序显示官方安装地址，不后台下载安装。
Windows 10 build 19041 or later Windows 10/11, x64, a compatible HomePod and WebView2 Runtime are required.
MSIX declares the VCLibs Desktop framework; Store deploys it. The standalone NSIS installer requires the system VC++ x64 runtime.
If WebView2 is absent, the app explains how to install it from Microsoft; there is no automatic download/installation.

## 对应源码 / Corresponding source

https://github.com/11thStKidzz/DeskAudio-Cast/releases/tag/v0.1.13
请下载 / Download **DeskAudioCast-0.1.13-corresponding-source.zip**。
该完整归档含锁定依赖及离线构建资料；GitHub 自动生成的源码 ZIP 不包含全部依赖，不能替代完整包。
The complete archive includes locked dependency sources and offline build inputs. GitHub's automatic source ZIP does not replace it.
按 / Follow [SOURCE_BUILDING.md](SOURCE_BUILDING.md). SHA-256 and source fingerprint accompany the release.

## 隐私、许可与支持 / Privacy, licensing and support

[完整双语隐私政策 / Full bilingual privacy policy](legal/PRIVACY.txt)。
应用在内存中处理声音并发送到所选设备，保留本地设置及数字诊断，不主动录制麦克风或自动上传音频。
The app processes audio in memory, sends it to the selected receiver and retains local settings and numeric diagnostics; no active microphone capture or automatic cloud-audio upload.

GNU GPL 第 3 版或更新版本；收费不限制适用开源权利。完整 [LICENSE](LICENSE) 和 [第三方声明 / Third-party notices](legal/THIRD_PARTY_NOTICES.txt) 随附。
GNU GPL version 3 or later; charging does not restrict applicable open-source rights.

独立修改自 AirSend，保留原作者 Pablo Díaz Ruipérez 及贡献者版权。协议来源和修改记录见 vendor/airplay2-gpl3、[MODIFICATIONS.md](MODIFICATIONS.md)。
Independent modification of AirSend; original author Pablo Díaz Ruipérez and contributor rights are retained.
不是 Apple 官方产品或认证产品；HomePod、AirPlay 仅用于兼容性说明，商标属于各权利人。
Not an official or Apple-certified product; HomePod and AirPlay describe compatibility only.

发布者 / Publisher: 11thStKidzz  
支持 / Support: woyaotingspotify@gmail.com
