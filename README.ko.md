# intwav

[English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md) | [Español](README.es.md) | [Deutsch](README.de.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md)

**정수 PCM 보호형 오디오 처리 도구** — 24bit PCM으로 디지털화된 아날로그 음원(레코드, 릴 테이프, 카세트)을 아카이빙하기 위한 도구입니다.

> 24bit PCM을 있는 그대로 지킵니다. 음질 개선이 아닌, 음질 보전.

intwav는 부동소수점 변환, 재양자화 및 리샘플링을 **전혀 수행하지 않고**, 정수 PCM의 검사, 트림 및 손실 없는 압축 아카이빙을 실행합니다. DAW가 아니므로 음질을 "개선"하지 않습니다. 캡처된 PCM 데이터를 원본 그대로 유지하며, 설명 가능하고 로그로 기록된 처리 경로를 통해 손실 없는 무손실 FLAC으로 저장합니다.

## 상태: v0.4

구현된 명령어:

| 명령어 | 목적 |
|---|---|
| `intwav info <in>`   | 포맷, 파라미터, 재생 시간, 피크, 클립 수 표시 |
| `intwav check <in>`  | 전체 검사: info + DC 오프셋 + 무음 감지 |
| `intwav peak <in>`   | 채널별 피크 레벨 (dBFS + 원본 값) |
| `intwav clips <in>`  | 클립(소리 깨짐) 샘플 카운트 |
| `intwav trim <in> [out] --from <ts> --to <ts>` | 샘플 값을 변경하지 않고 구간 추출 |
| `intwav split <in> --out <dir> (--cue <f> \| --by silence\|ab)` | 메타데이터를 유지하여 트랙 분할 (CUE 목록, 무음 감지 또는 A/B면) |
| `intwav gain <in> <out> --db <n>` | 고정소수점 게인 조절, 정수 dB (-96..=24). 양의 게인 (`+`)에는 `--allow-clipping`이 필요 |
| `intwav fade-in <in> <out> --duration <d>` | 선형 고정소수점 페이드 인 |
| `intwav fade-out <in> <out> --duration <d>` | 선형 고정소수점 페이드 아웃 |
| `intwav dc-correct <in> <out>` | 채널별 DC 오프셋 제거 |
| `intwav export16 <in> <out> [--dither tpdf]` | TPDF 디더링을 사용한 16-bit 파생 출력 (마스터링 용도가 아닙니다) |
| `intwav verify <a> [b]` | PCM 체크섬 계산 또는 두 파일이 동일한 PCM을 포함하는지 검증 |

타임스탬프는 `HH:MM:SS.mmm`, `MM:SS.mmm`, `SS.mmm` 또는 초 단위 숫자로 지정할 수 있으며, 시간 길이(duration)는 `5s` / `250ms` 형식도 지원합니다.
모든 처리 명령어는 `--output-format flac|wav`(기본값: 출력 확장자에서 유추, 지정되지 않은 경우 FLAC) 및 PCM SHA-256 체크섬과 처리 로그 해시를 포함하는 JSON 처리 보고서(§13/§22)를 생성하는 `--report <path>`를 허용합니다.

게인, 페이드, DC 보정 및 16-bit 디더링은 모두 **고정소수점 정수** 연산입니다. 게인 계수는 미리 계산된 Q31 테이블에서 가져오며(`pow` 함수 미사용), TPDF 디더링은 재현 가능한 `--seed`를 지정할 수 있는 정수 PRNG(의사 난수 생성기)를 사용합니다.

### 지원 포맷

* 입력: WAV 및 FLAC, 16/24/32-bit **정수** PCM, 모노 또는 스테레오.
* 출력: FLAC (기본값) 또는 WAV.
* 부동소수점 WAV, 압축 WAV, MP3/AAC/Opus, DSD, 멀티채널 음원은 명시적 오류 메시지와 함께 **거부**됩니다(절대 암묵적으로 변환되지 않습니다).

## 부동소수점 미사용 보증 (The float-free guarantee)

모든 샘플 연산은 `intwav-core` 내에서 처리됩니다. 이 크레이트는 `no_std` + `alloc`으로 의존성이 없으며 **부동소수점을 전혀 사용하지 않습니다**. dBFS 계산조차 고정소수점 정수 로그 근사를 사용하여 계산됩니다(오차 < 0.004 dB). FLAC 디코딩은 순수 Rust로 작성된 `claxon`을 사용하며, FLAC 인코딩은 외부 `flac` 바이너리에 위임하므로 libFLAC 내부의 부동소수점 분석이 당 프로세스에 들어오는 일이 없습니다.

`scripts/check-no-float.sh`는 CI에서 이를 강제합니다. 코어 소스의 부동소수점 구문을 스캔하고, 컴파일된 코어 객체를 역어셈블하여 부동소수점 연산 명령(x86-64 SSE/x87 또는 aarch64 FP)이 나타나면 빌드를 실패시킵니다.

## 아키텍처

```
crates/
  intwav-core     순수 정수 DSP: 분석, 윈도우 기반 무음 감지, dBFS, 슬라이싱, 게인/페이드/DC, TPDF 디더 (부동소수점 미사용 스캔 완료)
  intwav-codec    WAV (hound) + FLAC (claxon 디코딩 / flac-CLI 인코딩) 정수 입출력, 메타데이터, 헤더 탐지 (header probe)
  intwav-engine   CLI/GUI 공용 엔진: 작업 수행, 고정된 JSON 보고서, 코드화된 오류, 검증된 원자적 쓰기, 일회성 디코드 스크래치 파일 + 파형 피라미드, 비파괴 프로젝트 (.iwproj) + 실행 취소/렌더링 (부동소수점 미사용 소스)
  intwav-playback 미리보기 재생 (cpal): 정수 연산 체인 미리보기, 디바이스 경계에서만 부동소수점 사용 — 저장 경로 외부에 위치하며 부동소수점 미사용 스캔 대상 아님
  intwav-cli      `intwav` 바이너리: 엔진 위에서 동작하는 가벼운 프론트엔드
```

`intwav-engine` 크레이트는 GUI(Tauri + React)의 기반이 됩니다. 모든 작업은 동기식이자 호출자 주도형(진행률 알림 + 취소 지원)으로 이루어지며, 모든 쓰기 작업은 검증되고(`pcm_verified`), CLI와 GUI는 이 동일한 엔진을 그대로 공유합니다. `open_source`는 대용량 소스를 단 한 번 시크(seek) 가능한 스크래치 파일로 디코딩하는 동시에, 단일 패스로 파형과 PCM 해시를 생성합니다. `intwav-playback`은 이 스크래치 파일에서 미리보기 재생을 수행하며 내보내기(export) 때와 동일한 정수 연산 체인을 실행하고, 최종 오디오 디바이스 변환 시에만 부동소수점을 사용합니다(기본 샘플 레이트 우선, 부동소수점 리샘플링은 대체 수단).

## GUI (Tauri + React) — 미리보기

데스크톱 GUI는 `app/`에 위치합니다. 엔진을 명령어로 노출하는 Tauri v2 백엔드(`src-tauri/`, 무거운 빌드 과정이 CI 속도를 저하시키지 않도록 코어 워크스페이스에서 **분리**된 크레이트)와 React + TypeScript 프론트엔드(기본 일본어, 2개 국어 지원)로 구성됩니다. `open_source`(일회성 디코드 스크래치 파일 + 파형)를 통해 WAV/FLAC를 열고, 파형 및 고정 보고서 팩트에 의해 제어되는 **Integer-Safe(정수 안전)** 상태 패널을 표시하며, 실시간 진행률 표시와 취소 기능이 지원되는 trim/gain/export16/verify 작업을 실행합니다 — 이 모든 과정은 CLI가 사용하는 동일한 엔진을 통해 수행됩니다.

```bash
cd app
npm install
npm run tauri dev     # 개발 (데스크톱 세션 필요)
npm run tauri build   # 서명된 앱 번들 생성 (macOS/Windows/Linux)
```

프론트엔드는 `npm run build`를 통해 헤드리스(headless) 빌드가 가능하며, 백엔드는 `app/src-tauri` 내에서 `cargo check`로 컴파일 및 검증을 진행할 수 있습니다.

## 빌드 및 테스트

```bash
cargo build --release          # 바이너리 생성 경로: target/release/intwav
cargo test --workspace         # 유닛 테스트 + E2E 테스트
bash scripts/check-no-float.sh # 부동소수점 미사용 보증 검증
```

FLAC 출력을 위해서는 커맨드라인 도구 `flac`이 필요합니다.

## 라이선스
Apache-2.0
