#!/usr/bin/env python3
import os
import subprocess
import sys
from pathlib import Path

def main():
    if len(sys.argv) != 2:
        print(f"用法: {sys.argv[0]} [路径]")
        sys.exit(1)

    target_path = Path(sys.argv[1])
    if not target_path.exists():
        print(f"错误: 路径不存在: {target_path}")
        sys.exit(1)

    # 构建exiftool命令
    cmd = [
        "exiftool",
        "-FileCreateDate<DateTimeOriginal",
        "-FileModifyDate<DateTimeOriginal",
        "-v2",
        "-r",  # 递归处理子目录
        str(target_path)
    ]

    try:
        # 执行命令
        result = subprocess.run(
            cmd,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )
        print("操作成功完成！")
        print(result.stdout)
    except subprocess.CalledProcessError as e:
        print(f"错误: 执行命令失败: {e.stderr}")
        sys.exit(1)
    except Exception as e:
        print(f"错误: 发生未知错误: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()
