#!/usr/bin/env python3
import os
import subprocess
import json
from pathlib import Path
import sys
import shutil

def get_video_rotation(video_path):
    """获取视频的旋转角度"""
    # 先尝试通过常规元数据获取旋转信息
    cmd = [
        'ffprobe',
        '-v', 'error',
        '-select_streams', 'v:0',
        '-show_entries', 'stream_tags=rotate',
        '-of', 'json',
        str(video_path)
    ]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        info = json.loads(result.stdout)
        streams = info.get('streams', [{}])
        tags = streams[0].get('tags', {})
        rotation = int(tags.get('rotate', 0))
        if rotation in (90, 180, 270):
            return rotation
    except (subprocess.CalledProcessError, json.JSONDecodeError, ValueError):
        pass
    
    # 如果常规方法失败，则尝试从显示矩阵中提取旋转信息
    cmd = [
        'ffprobe',
        '-v', 'error',
        '-select_streams', 'v:0',
        '-show_entries', 'stream_side_data_list',
        '-of', 'json',
        str(video_path)
    ]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        info = json.loads(result.stdout)
        streams = info.get('streams', [{}])
        side_data_list = streams[0].get('side_data_list', [])
        
        for side_data in side_data_list:
            side_data_type = side_data.get('side_data_type')
            if side_data_type == 'Display Matrix' or side_data_type == 'displaymatrix':
                rotation = side_data['rotation']
                if rotation == -90:
                    return 90
                if rotation == 90:
                    return 270
                if rotation == 180:
                    return 180
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        pass
    
    return 0

def transcode_with_rotation(video_path):
    """根据旋转角度转码视频"""
    input_path = Path(video_path)
    output_path = input_path.with_name(f"{input_path.stem}_trans{input_path.suffix}")

    cmd = [
        'ffmpeg',
        '-hide_banner', '-loglevel', 'error',
        '-i', str(input_path),
        '-c:a', 'copy',  # 音频流直接复制，不重新编码
        '-crf', '23',  # 视频质量参数，可根据需要调整
        '-preset', 'medium',  # 编码速度与压缩比的平衡，可调整
        '-y',  # 覆盖已存在的输出文件
        str(output_path)
    ]
    
    try:
        subprocess.run(cmd, check=True)
        print(f"成功转码: {input_path} -> {output_path}")
    except subprocess.CalledProcessError as e:
        print(f"转码失败: {input_path}, 错误: {e}")
        raise

    os.remove(input_path)
    os.rename(output_path, input_path)

def main(path):
    # 支持的视频格式
    VIDEO_EXTENSIONS = {
        '.mp4', '.mkv', '.avi', '.mov', '.wmv', '.flv', '.webm', '.m4v', '.3gp'
    }
    
    # 获取当前目录下的所有视频文件
    current_dir = Path(path)
    video_files = [
        f for f in current_dir.iterdir() 
        if f.is_file() and f.suffix.lower() in VIDEO_EXTENSIONS
    ]
    
    if not video_files:
        print("当前目录下未找到视频文件")
        return
    
    for video_file in video_files:
        print(f"检查: {video_file}")
        rotation = get_video_rotation(video_file)
        
        if rotation:
            transcode_with_rotation(video_file)

if __name__ == "__main__":
    if len(sys.argv) > 1:
        main(sys.argv[1])
    else:
        main('.')
