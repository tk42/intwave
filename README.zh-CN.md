# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**整数 PCM 保护型音频处理工具** — 用于将数字化为 24-bit PCM 的模拟音源（唱片、磁带、卡带）进行无损存档。

> 原汁原味保留 24-bit PCM。不是音质修饰，而是音质珍藏与保全。

intwav 能够在**不进行**浮点转换、重新量化或重采样的情况下，对整数 PCM 进行检查、裁剪和无损压缩存档。它不是 DAW（数字音频工作站），不会对音频进行所谓的“改善” — 它完全依照录入时的状态保留 PCM 原始数据，并以无损 FLAC 格式保存，同时提供可解释且完整记录的日志处理路径。

## 状态: v0.4

已实现的命令：

| 命令 | 用途 |
|---|---|
| `intwav info <in>`   | 格式、参数、时长、峰值 (peak)、削波计数 (clip count) |
| `intwav check <in>`  | 完整检查：info + 直流偏移 (DC offset) + 静音检测 |
| `intwav peak <in>`   | 各通道峰值电平 (dBFS + 原始值) |
| `intwav clips <in>`  | 削波样本计数 |
| `intwav trim <in> [out] --from <ts> --to <ts>` | 提取区间，保持样本值完全不变 |
| `intwav split <in> --out <dir> (--cue <f> \| --by silence\|ab)` | 拆分为音轨（CUE 列表、静音检测或 A/B 面）并保留元数据 |
| `intwav gain <in> <out> --db <n>` | 定点增益调节，整数 dB (-96..=24)；正向增益 (`+`) 需要 `--allow-clipping` 参数 |
| `intwav fade-in <in> <out> --duration <d>` | 线性定点淡入 |
| `intwav fade-out <in> <out> --duration <d>` | 线性定点淡出 |
| `intwav dc-correct <in> <out>` | 移除各通道的直流偏移 (DC offset) |
| `intwav export16 <in> <out> [--dither tpdf]` | 使用 TPDF 抖动的 16-bit 衍生输出（非母带用途） |
| `intwav verify <a> [b]` | 计算 PCM 校验和，或证明两个文件包含完全相同的 PCM 数据 |

时间戳支持 `HH:MM:SS.mmm`、`MM:SS.mmm`、`SS.mmm` 或纯秒数格式；时长同时也支持 `5s` / `250ms` 格式。
所有音频处理命令均支持 `--output-format flac|wav`（默认：根据输出文件扩展名推断，否则使用 FLAC）以及用于生成 JSON 处理报告（§13/§22）的 `--report <path>` 参数，报告内包含 PCM SHA-256 校验和与处理日志哈希值。

增益、淡入淡出、直流校正和 16-bit 抖动全部都是**定点整数**运算。增益系数来自预先计算好的 Q31 表（不使用 `pow` 函数）；TPDF 抖动使用带有可复现 `--seed` 种子的整数伪随机数生成器 (PRNG)。

### 支持格式

* 输入：WAV 和 FLAC，16/24/32-bit **整数** PCM，单声道或立体声。
* 输出：FLAC（默认）或 WAV。
* 浮点 WAV、压缩 WAV、MP3/AAC/Opus、DSD 及多声道音频被明确提示错误并**拒绝** — 绝不进行任何暗中静默转换。

## 无浮点运算保证 (The float-free guarantee)

所有采样点计算均处于 `intwav-core` 中，该 crate 采用 `no_std` + `alloc`，零外部依赖，并且**不使用任何浮点数** — 甚至连 dBFS 的计算都是通过定点整数对数近似处理完成的（误差 < 0.004 dB）。FLAC 独立解码采用纯 Rust 编写的 `claxon`；FLAC 编码则委托给外部的 `flac` 二进制程序执行，因而 libFLAC 内部的浮点分析绝不会进入本进程中。

`scripts/check-no-float.sh` 在 CI（持续集成）中强制执行此标准：它扫描核心源代码中的浮点结构，并对编译后的核心目标文件进行反汇编；如果出现任何浮点算术指令（x86-64 SSE/x87 或 aarch64 FP），构建将被直接中止。

## 架构

```
crates/
  intwav-core   纯整数 DSP：分析、dBFS、切片、增益/淡入淡出/直流校正、TPDF 抖动（已扫描确保无浮点）
  intwav-codec  WAV (hound) + FLAC (claxon 解码 / flac-CLI 编码) 整数 I/O + 元数据
  intwav-cli    `intwav` 二进制程序：命令行解析、文件 I/O、JSON 报告、校验和
```

## 构建与测试

```bash
cargo build --release          # 编译程序生成于 target/release/intwav
cargo test --workspace         # 单元测试 + 端口到端口（E2E）测试
bash scripts/check-no-float.sh # 验证无浮点运算保证
```

输出 FLAC 格式需要系统安装有 `flac` 命令行工具。

## 许可证
Apache-2.0
