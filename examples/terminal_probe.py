#!/usr/bin/env python3
"""Print terminal fixtures. Run inside DevHub; inspect visually, not a pass/fail test."""
import argparse
import sys
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--alternate", action="store_true")
    parser.add_argument("--lines", type=int, default=0)
    args = parser.parse_args()
    print("ASCII 0123456789  |  中文：目录、会话、终端  |  combining: e\u0301")
    print("\033[1mBold\033[0m  \033[3mItalic\033[0m  \033[4mUnderline\033[0m")
    for i in range(16):
        print(f"\033[48;5;{i}m {i:02} \033[0m", end="")
    print("\n\033[38;2;100;190;240mTrue color\033[0m")
    print("使用 Shift+滚轮选择回滚；Ctrl+Shift+C/V 复制粘贴；调整窗口尺寸。")
    for i in range(max(0, min(args.lines, 100_000))):
        print(f"line {i:06} 中文 terminal output")
    if args.alternate:
        try:
            sys.stdout.write("\033[?1049h\033[2J\033[HAlternative screen: returning in 2 seconds")
            sys.stdout.flush()
            time.sleep(2)
        finally:
            sys.stdout.write("\033[?1049l")
            sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
