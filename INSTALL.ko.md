# nmtk 설치하기

[English](INSTALL.md)

터미널에 아래 한 줄을 붙여 넣으세요. 필요한 것을 전부 설치하고 바로 실행됩니다.

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git && cd needmoretruthknowledge && ./install.sh
```

이게 전부입니다. 중간에 아무것도 묻지 않습니다.

## 그 한 줄이 하는 일

1. 이 저장소를 `needmoretruthknowledge` 폴더로 받습니다.
2. Rust 1.98 이상이 있는지 봅니다. 없으면 `rustup`으로 설치합니다 — `~/.rustup`과 `~/.cargo`에만
   들어가고 시스템의 다른 곳은 건드리지 않습니다.
3. nmtk를 빌드합니다. 처음에는 1분쯤 걸리고, 이 컴퓨터의 모든 코어를 씁니다.
4. 프로그램을 `~/.local/bin/nmtk`에 놓습니다.
5. 실행합니다.

다음부터는 `nmtk`만 치면 실행됩니다. 셸이 `command not found`라고 하면 `~/.local/bin`이 `PATH`에
없다는 뜻이고, 설치 프로그램이 그것을 고치는 한 줄을 알려 줍니다.

## 되는 시스템

| | |
|---|---|
| **페도라** | 됩니다. 다른 것을 설치할 필요 없습니다 |
| **우분투** | 됩니다. 다른 것을 설치할 필요 없습니다 |
| 그 밖의 리눅스 | 될 것입니다 — nmtk는 Rust 밖의 라이브러리를 쓰지 않습니다 |
| macOS, 윈도우 | 아직 안 됩니다 |

터미널이 최소 **가로 80칸, 세로 24줄**은 돼야 하고, 한국어로 읽으려면 한글이 있는 터미널 글꼴이
필요합니다(대부분 있습니다. 페도라와 우분투는 기본으로 깔려 있습니다).

## 직접 하고 싶다면

```sh
git clone https://github.com/needmoretruth/needmoretruthknowledge.git
cd needmoretruthknowledge
cargo build --release
./target/release/nmtk
```

어디서나 `nmtk`로 실행하고 싶다면:

```sh
cargo install --path crates/nmtk
nmtk
```

## 잘 안 될 때

**`curl: command not found`** — curl을 설치하세요. 페도라는 `sudo dnf install curl`,
우분투는 `sudo apt install curl`.

**`error: linker 'cc' not found`** — Rust에 링커가 필요합니다.
페도라는 `sudo dnf install gcc`, 우분투는 `sudo apt install build-essential`.

**빌드 도중에 강제 종료됨** — 메모리가 모자란 것입니다. 한 번에 하나씩 빌드하세요:
`cargo build --release -j 1`.

**화면이 네모와 물음표투성이** — 터미널 글꼴에 선 문자나 한글이 없는 것입니다. DejaVu Sans Mono,
Noto Sans Mono, JetBrains Mono 중 아무거나 쓰면 됩니다.

**`This screen needs 80x24`** — 창을 키우거나 글자 크기를 줄이세요.

## 지우기

```sh
rm ~/.local/bin/nmtk                      # 프로그램
rm -rf ~/.config/nmtk                     # 내 설정
rm -rf /경로/needmoretruthknowledge        # 소스
```

설치 프로그램이 Rust를 깔아 준 경우, Rust는 `~/.rustup`과 `~/.cargo`에 있고
`rustup self uninstall`로 지웁니다.

nmtk는 그 밖의 어디에도 아무것도 쓰지 않고, 네트워크도 쓰지 않았습니다.
