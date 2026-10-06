#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../../.."
export DISPLAY=:0
if pgrep -f '^.*/release/pokemoon[^ /]*( |$)' >/dev/null; then echo 'Moon already running';exit 1;fi
task_dir="$PWD/static-recomp-work/nativeization/remove-zakuro/moon-native"
bash "$task_dir/run-native.sh" > "$task_dir/smoke.log" 2>&1 &
task_pid=$!
trap 'if kill -0 "$task_pid" 2>/dev/null;then kill -TERM "$task_pid";wait "$task_pid" || true;fi' EXIT
task_window=''
for ((task_try=0;task_try<150;task_try++));do
 task_window=$(xdotool search --pid "$task_pid" --name '^Pokémon Moon$' 2>/dev/null | head -1 || true)
 [[ -n "$task_window" ]] && break
 kill -0 "$task_pid" 2>/dev/null || exit 1
 sleep .2
done
[[ -n "$task_window" ]] || exit 1
sleep 8
for task_key in x x x;do
 xdotool keydown --window "$task_window" "$task_key"
 sleep .3
 xdotool keyup --window "$task_window" "$task_key"
 sleep 3
done
sleep 10
timeout 10s import -window "$task_window" "$task_dir/smoke-before.png"
xdotool keydown --window "$task_window" l
sleep .6
xdotool keyup --window "$task_window" l
sleep 1
timeout 10s import -window "$task_window" "$task_dir/smoke-after.png"
wmctrl -ic "$task_window"
wait "$task_pid"
trap - EXIT
echo 'smoke_exit=0'
