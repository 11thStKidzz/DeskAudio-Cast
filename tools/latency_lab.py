"""GPL-3.0-or-later. Offline probe generation and SAME-CLOCK stereo measurement.

Never records, connects to a device, or interprets a slider as measured latency.
Only standard-library dependencies. Channel numbers are zero based.
"""
import argparse
from array import array
import json
import math
from pathlib import Path
import statistics
import sys
import wave

ONSETS_MS = (1000, 2310, 4080, 5530, 7620, 9190, 11430, 13370)
DURATION_MS = 15000


def probe_samples(rate=48000):
    samples = array('h', [0]) * (rate * DURATION_MS // 1000)
    length = rate * 25 // 1000
    for onset in ONSETS_MS:
        start = rate * onset // 1000
        for i in range(length):
            t = i / rate
            envelope = math.sin(math.pi * i / (length - 1)) ** 2
            phase = 2 * math.pi * (2000 * t + 0.5 * 160000 * t * t)
            samples[start + i] = round(4000 * envelope * math.sin(phase))
    return samples


def write_pcm(path, samples, rate, channels):
    if sys.byteorder != 'little':
        samples = array('h', samples)
        samples.byteswap()
    # Exclusive creation: never overwrite a recording or earlier report.
    with open(path, 'xb') as output:
        with wave.open(output, 'wb') as wav:
            wav.setnchannels(channels)
            wav.setsampwidth(2)
            wav.setframerate(rate)
            wav.writeframes(samples.tobytes())


def read_envelopes(path, reference, receiver):
    if reference == receiver or min(reference, receiver) < 0:
        raise ValueError('参考声道与接收声道必须不同，且编号从 0 开始')
    if Path(path).stat().st_size > 100_000_000:
        raise ValueError('文件超过 100 MB，请截取测试片段')
    with wave.open(str(path), 'rb') as wav:
        channels, width, rate, frames = (wav.getnchannels(), wav.getsampwidth(),
                                         wav.getframerate(), wav.getnframes())
        if width != 2 or not 8000 <= rate <= 192000 or max(reference, receiver) >= channels:
            raise ValueError('需要 16 位 PCM WAV，8–192 kHz，至少两个独立声道')
        if not 14 <= frames / rate <= 120:
            raise ValueError('录音必须为 14–120 秒，完整包含测试音')
        step = max(1, rate // 1000)
        envelopes = [[], []]
        clipped = [0, 0]
        for _ in range(0, frames, step):
            block = array('h')
            block.frombytes(wav.readframes(step))
            if sys.byteorder != 'little':
                block.byteswap()
            for k, channel in enumerate((reference, receiver)):
                values = block[channel::channels]
                if not values:
                    raise ValueError('WAV 数据不完整')
                clipped[k] += sum(abs(v) >= 32760 for v in values)
                envelopes[k].append(math.sqrt(sum(v*v for v in values) / len(values)))
    if any(clipped):
        raise ValueError('检测到削波，请降低录音增益后重新测量')
    return envelopes, step * 1000 / rate


def detect(envelope, resolution):
    peak = max(envelope, default=0)
    floor = statistics.median(envelope)
    if peak < 32 or peak < floor * 8:
        raise ValueError('测试音过弱或背景噪声过高')
    threshold = max(peak * 0.18, floor * 5)
    events = []
    i = 0
    while i < len(envelope):
        if envelope[i] < threshold:
            i += 1
            continue
        start = i
        while i < len(envelope) and envelope[i] >= threshold:
            i += 1
        duration = (i - start) * resolution
        if duration >= 4:
            if not 4 <= duration <= 65:
                raise ValueError('检测到非测试脉冲，请勿混入音乐或其他声音')
            events.append(start * resolution)
    if not 5 <= len(events) <= len(ONSETS_MS):
        raise ValueError('有效测试脉冲数量异常，可能漏录、串音或混入其他声音')
    return events


def align_pattern(events):
    # Match irregular intervals, not ordinal positions: missing pulses cannot
    # silently shift all subsequent pairings.
    candidates = {}
    for event in events:
        for onset in ONSETS_MS:
            offset = event - onset
            mapping = {}
            for actual in events:
                distances = [abs(actual - offset - expected) for expected in ONSETS_MS]
                index = min(range(len(distances)), key=distances.__getitem__)
                if distances[index] <= 12 and index not in mapping:
                    mapping[index] = actual
            if len(mapping) == len(events):
                candidates[tuple(sorted(mapping))] = mapping
    if len(candidates) != 1:
        raise ValueError('无法唯一匹配测试序列，请检查录音声道与测试片段')
    return next(iter(candidates.values()))


def analyze(path, reference=0, receiver=1, kind='acoustic-relative'):
    envelopes, resolution = read_envelopes(path, reference, receiver)
    mappings = [align_pattern(detect(e, resolution)) for e in envelopes]
    indices = sorted(mappings[0].keys() & mappings[1].keys())
    if len(indices) < 5:
        raise ValueError('可配对脉冲不足，不能报告可靠延迟')
    delays = [mappings[1][i] - mappings[0][i] for i in indices]
    if min(delays) < 0 or max(delays) > 5000:
        raise ValueError('延迟超出 0–5000 ms，请检查声道顺序')
    ordered = sorted(delays)
    return {
        'schema_version': 1, 'measurement_kind': kind,
        'metric': 'receiver_minus_reference_onset_ms',
        'status': 'complete' if len(indices) == len(ONSETS_MS) else 'incomplete',
        'paired_pulses': len(indices), 'expected_pulses': len(ONSETS_MS),
        'missing_reference': len(ONSETS_MS) - len(mappings[0]),
        'missing_receiver': len(ONSETS_MS) - len(mappings[1]),
        'median_ms': round(statistics.median(delays), 3),
        'p95_ms': round(ordered[math.ceil(len(ordered)*0.95)-1], 3),
        'min_ms': round(min(delays), 3), 'max_ms': round(max(delays), 3),
        'spread_ms': round(max(delays)-min(delays), 3),
        'analysis_window_ms': round(resolution, 4),
        'absolute_latency_verified': False,
        'limitations': ['Requires isolated channels on one recorder clock',
                        'Reference path and acoustic travel are not calibrated',
                        'Envelope onset estimate is not sample-accurate',
                        'Does not assess sound quality or verify HomePod identity'],
    }


def main():
    parser = argparse.ArgumentParser(description='离线延迟测量；不会录音或连接设备')
    sub = parser.add_subparsers(dest='command', required=True)
    generate = sub.add_parser('generate', help='生成 15 秒轻声测试音，请勿调高音量')
    generate.add_argument('output')
    measure = sub.add_parser('analyze', help='分析同一录音的独立参考/接收声道')
    measure.add_argument('recording')
    measure.add_argument('--reference-channel', type=int, default=0)
    measure.add_argument('--receiver-channel', type=int, default=1)
    measure.add_argument('--kind', choices=['acoustic-relative', 'electrical-relative',
                                          'synthetic-validation'], required=True)
    measure.add_argument('--output', required=True)
    args = parser.parse_args()
    try:
        if args.command == 'generate':
            write_pcm(args.output, probe_samples(), 48000, 1)
            print('已生成合成测试音；未播放、未录音。')
        else:
            result = analyze(args.recording, args.reference_channel, args.receiver_channel, args.kind)
            with open(args.output, 'x', encoding='utf-8') as output:
                json.dump(result, output, ensure_ascii=False, indent=2)
            print(json.dumps(result, ensure_ascii=False, indent=2))
    except (ValueError, OSError, wave.Error, EOFError) as error:
        parser.exit(2, f'测量未完成：{error}\n')


if __name__ == '__main__':
    main()
