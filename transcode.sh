#!/usr/bin/env sh

# TODO https://github.com/arut/nginx-rtmp-module/wiki/Exec-wrapper-in-bash

name=$1

/usr/bin/ffmpeg -i rtmp://localhost:1935/src/$name \
    -vf scale=-2:240 -vcodec libx264 -preset faster -b:v 200K -acodec aac -ar 44100 -ac 1 -f flv rtmp://localhost/hls/${name}_240 \
    -vf scale=-2:480 -vcodec libx264 -preset fast -b:v 1000K -acodec aac -ar 44100 -ac 2 -f flv rtmp://localhost/hls/${name}_480 \
    -vf scale=-2:720 -vcodec libx264 -preset fast -b:v 1300K -acodec aac -ar 44100 -ac 2 -f flv rtmp://localhost/hls/${name}_720 2>>/tmp/ffmpeg-$name.log
