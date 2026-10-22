# DeskAudio Cast GPLv3 candidate modifications

Date: 2026-10-21. Changes are offered under GPL-3.0-or-later.
Upstream copyright and LICENSE are retained without replacement.
Source: lmcgartland/airplay2-rs, f824007798f71605307384829800273a4dbbcee2.

- Workspace selects only the nine protocol crates and the resampler. Bluetooth/TUI are not imported.
- Windows UDP portability guard; Windows audio scheduling uses documented Avrt APIs.
- New live worker and numeric diagnostics implement the application interface and its three bounded buffer profiles.
- The worker uses upstream GPLv3 RTP, authenticated encryption, retransmit history and ALAC encoding.
- No resampling/EQ/spatial processing is introduced in the live path. ALAC encodes the original i16 PCM samples.
- Reconstructed shared-timeline transport and standard RTSP volume query; physical-device behavior remains to be tested.
- Removed the FDK-AAC encoder and dependency. Former AAC-specific tests assert rejection, not AAC output. ALAC tests retained.
- Persistent plaintext identities and their private pair-verify path removed; public persistence APIs return Unsupported.
- Protocol logging emits only static event identifiers at all log levels. Typed application health metrics remain available.
- The old GPLv2 fork is not copied as the candidate protocol source. No clean-room certification is asserted.

2026-10-21 default-source follow-up: independent full library regression found the unused EncryptedChannel wrapper double-framed ControlCipher output. Reuse authenticated LE/plaintext-length framing, bound raw blocks, validate tags/truncation, add multiblock tests. The production client uses ControlCipher directly; ALAC/live worker/buffer settings are unchanged. Replace obsolete persistence-success tests with explicit refusal/no-file-write/leave-existing-file-untouched checks; do not re-enable plaintext storage to satisfy old tests.

This is a local test implementation, not a publishable or legally cleared distribution.
# 2026-10-22：实时期限和数值诊断

仅在原定发送期限已经到达后才将空输入计为欠载，避免在期限前立即触发重新缓冲；不增加任何三档缓冲参数，不更改 ALAC 编码、样本内容或回退规则。
新增最多 64 次欠载的相对时间/数据块供给与消费计数/编码耗时/调度迟到，不包含音频或凭据；将快照百分位排序移到音频共享锁外。
这些改动修复可由测试重现的提前判断问题，不宣称已经确定任何用户现场爆音的全部原因。

