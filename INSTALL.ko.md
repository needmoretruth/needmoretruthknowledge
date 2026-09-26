# nmtk 설치하기

[English](INSTALL.md)

페도라나 우분투의 터미널에 아래 한 줄을 붙여 넣으세요. nmtk에 필요한 것을 설치하고 바로 실행합니다.

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git && cd needmoretruthknowledge && ./install.sh
```

이게 전부입니다. 중간에 아무것도 묻지 않습니다. 예외는 하나뿐입니다. 시스템에 C 컴파일러가 없거나 Rust를
내려받을 도구가 없으면 `sudo`가 비밀번호를 물을 수 있습니다.

## 그 한 줄이 하는 일

1. 이 저장소를 `needmoretruthknowledge` 폴더로 받습니다.
2. Rust에 필요한 C 링커(`cc`)가 있는지 봅니다. 없으면 설치합니다. 우분투와 데비안은 `apt-get`으로
   `build-essential`을, 페도라는 `dnf`로 `gcc`를 설치합니다. 시스템 전체에 설치할 수 있는 것은 이것과
   `curl`(3단계) 둘뿐이고, root 권한이 필요한 단계도 이 둘뿐입니다.
3. 이 저장소가 쓰는 Rust 1.98.1이 있는지 확인합니다. `rustup`이 있으면 그것으로 이 버전을 받습니다.
   Rust가 아예 없으면 `rustup`과 Rust 1.98.1을 `~/.rustup`과 `~/.cargo`(`RUSTUP_HOME`과 `CARGO_HOME`을
   정해 두었다면 그곳)에만 설치합니다. 내려받을 때는 `curl`을 쓰고, `curl`이 없으면 `wget`을 씁니다.
   둘 다 없으면 링커와 같은 방법으로 `curl`을 먼저 설치합니다.
4. nmtk를 빌드합니다. 처음에는 1분쯤 걸리고, 이 컴퓨터의 모든 코어를 씁니다.
5. 프로그램을 `~/.local/bin/nmtk`에 놓습니다.
6. 실행합니다. 단, 터미널에서 돌릴 때만 그렇습니다. 파이프로 넘기거나 CI에서 돌리면 실행하는 방법만
   알려 줍니다.

다음부터는 `nmtk`만 치면 실행됩니다. 셸이 `command not found`라고 하면 `~/.local/bin`이 `PATH`에
없다는 뜻이고, 설치 프로그램이 그것을 고치는 한 줄을 알려 줍니다.

다시 돌려도 괜찮습니다. 이미 있는 것은 건너뜁니다.

## 옵션

| | |
|---|---|
| `./install.sh --no-run` | 빌드하고 설치만 합니다. nmtk를 실행하지 않습니다 |
| `NMTK_NO_RUN=1 ./install.sh` | 위와 같습니다. 환경 변수로 쓰는 방법입니다 |
| `./install.sh --help` | 스크립트가 하는 일을 보여 주고 끝냅니다 |

## 되는 시스템

| | |
|---|---|
| **페도라** | 됩니다. `gcc`와 `curl`이 없으면 설치합니다 |
| **우분투** | 됩니다. `build-essential`과 `curl`이 없으면 설치합니다 |
| 그 밖의 리눅스 | `cc`와, `curl`이나 `wget`이 이미 있으면 될 것입니다 — nmtk는 Rust 밖의 라이브러리를 쓰지 않습니다 |
| macOS, 윈도우 | 아직 안 됩니다 |

터미널이 최소 **가로 80칸, 세로 24줄**은 돼야 하고, 한국어로 읽으려면 한글이 있는 터미널 글꼴이
필요합니다(대부분 있습니다. 페도라와 우분투는 기본으로 깔려 있습니다).

## 직접 하고 싶다면

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git
cd needmoretruthknowledge
cargo build --release --locked
./target/release/nmtk
```

어디서나 `nmtk`로 실행하고 싶다면:

```sh
cargo install --locked --path crates/nmtk
nmtk
```

## 잘 안 될 때

**`error: linker 'cc' not found`** — Rust에 링커가 필요합니다. 설치 프로그램이 설치를 시도하는데,
그래도 안 됐다면 페도라는 `sudo dnf install -y gcc`, 우분투는
`sudo apt-get update && sudo apt-get install -y build-essential`을 실행하세요. 그다음 `./install.sh`를
다시 돌리세요.

**`installing it needs root. There is no sudo here`** — root로 로그인해서 설치 프로그램이 알려 준
명령을 실행하고, 다시 내 계정으로 `./install.sh`를 돌리세요.

**`Installing a C linker (cc) did not work`** 또는 **`Installing curl did not work`** — `sudo`가
거절했거나, 터미널 없이 돌려서 비밀번호를 물을 수 없었거나, 패키지 관리자가 실패한 것입니다. 설치
프로그램이 알려 준 명령을 직접 실행하세요. 우분투에서 `apt-get update`가 실패하면 경고만 하고
(`apt-get update reported a problem; trying the install anyway`) 설치를 계속합니다. 그 뒤에 설치가
실패했다면 대개 패키지 저장소 하나가 고장 난 것입니다.

**`Could not download rustup`** — `https://sh.rustup.rs`에서 내려받지 못한 것이고, 대개 네트워크가
없어서입니다. 연결을 확인하고 `./install.sh`를 다시 돌리세요.

**빌드가 더 새로운 `rustc`가 필요하다며 실패함** — 이 저장소는 `rust-toolchain.toml`에서 Rust 1.98.1을
고정합니다. `rustup`은 이 파일을 읽지만, 패키지 관리자로 설치한 Rust는 읽지 않습니다.
[rustup.rs](https://rustup.rs)에서 `rustup`을 설치하거나, 이미 있다면 `rustup update`를 실행하세요.
그다음 `./install.sh`를 다시 돌리세요.

**설치 뒤에 `cargo: command not found`** — rustup은 cargo를 `~/.cargo/bin`(또는 `$CARGO_HOME/bin`)에
두는데, 새로 연 셸만 그것을 봅니다. 터미널을 새로 열거나 `. ~/.cargo/env`를 실행하세요.

**`nmtk: command not found`** — `~/.local/bin`이 `PATH`에 없는 것입니다. 설치 프로그램이 알려 준
`echo 'export PATH=…'` 줄을 실행하고 터미널을 새로 여세요. 아니면 전체 경로로 실행하세요:
`~/.local/bin/nmtk`.

**빌드 도중에 강제 종료됨** — 메모리가 모자란 것입니다. 한 번에 하나씩 빌드하세요:
`cargo build --release --locked -j 1`.

**화면이 네모와 물음표투성이** — 터미널 글꼴에 선 문자나 한글이 없는 것입니다. DejaVu Sans Mono,
Noto Sans Mono, JetBrains Mono 중 아무거나 쓰면 됩니다.

**`This terminal is 60x20. nmtk needs 80x24`** 또는 **`이 화면은 80x24가 필요합니다. 터미널을 키워
주세요.`** — 창을 키우거나 글자 크기를 줄이세요. 설치 프로그램은 경고만 합니다. nmtk는 창이 충분히 커질
때까지 이 문구를 보여 주고, 커지면 하던 자리에서 이어서 진행합니다.

## 지우기

```sh
rm ~/.local/bin/nmtk                      # 프로그램
rm -rf ~/.config/nmtk                     # 내 설정
rm -rf /경로/needmoretruthknowledge        # 소스
```

`XDG_CONFIG_HOME`을 설정해 두었다면 설정은 `$XDG_CONFIG_HOME/nmtk`에 있습니다.

설치 프로그램이 Rust를 깔아 준 경우, Rust는 `~/.rustup`과 `~/.cargo`(또는 `RUSTUP_HOME`과
`CARGO_HOME`)에 있고 `rustup self uninstall`로 지웁니다. C 링커와 `curl`을 설치했다면 그것은
보통의 시스템 패키지입니다. 그대로 두거나 평소처럼 `dnf`나 `apt-get`으로 지우세요.

nmtk는 그 밖의 어디에도 아무것도 쓰지 않고, 네트워크도 쓰지 않습니다.
