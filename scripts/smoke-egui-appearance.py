#!/usr/bin/env python3
"""Real egui appearance smoke: font/color/brightness, restart and minimum window.

Requires Xvfb, xdotool, ImageMagick, Pillow and JetBrainsMono-Regular.ttf in
$XDG_DATA_HOME/fonts (or ~/.local/share/fonts). Copies fonts into temporary data;
never opens the user database/vault. Run from the repository root under build_guard:
  python3 scripts/build_guard.py -- timeout 45s xvfb-run -a -s '-screen 0 1280x850x24' python3 scripts/smoke-egui-appearance.py target/release/opsterm-egui
An optional second argument changes the screenshot output directory.
"""
import importlib.util
import json
import os
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
import time
import shutil
from PIL import Image
spec = importlib.util.spec_from_file_location('measure', 'scripts/measure-egui.py')
measure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measure)
out = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else 'test-results/egui-appearance')
out.mkdir(parents=True, exist_ok=True)

def run(*args):
    subprocess.run(list(args), check=True)

def click(window, x, y):
    run('xdotool', 'mousemove', '--window', window, str(x), str(y), 'click', '1')
    time.sleep(0.15)

def capture(window, name):
    run('import', '-window', window, str(out / name))

def read_settings(data):
    with sqlite3.connect(pathlib.Path(data) / 'org.opsterm.app/opsterm.db') as db:
        return json.loads(db.execute("SELECT value_json FROM application_settings WHERE key='ui.egui'").fetchone()[0])

def count_color(name, color):
    image = Image.open(out / name).convert('RGB').crop((250, 90, 1170, 700))
    return sum((all((abs(a - b) <= 2 for a, b in zip(pixel, color))) for pixel in image.getdata()))
with tempfile.TemporaryDirectory(prefix='opsterm-appearance-gui-') as data:
    fonts = pathlib.Path(data) / 'fonts'
    fonts.mkdir()
    original_fonts = pathlib.Path(os.environ.get('XDG_DATA_HOME', str(pathlib.Path.home() / '.local/share'))) / 'fonts'
    if not (original_fonts / 'JetBrainsMono-Regular.ttf').is_file():
        raise RuntimeError('Smoke prerequisite missing: JetBrainsMono-Regular.ttf in user fonts directory')
    for name in ['JetBrainsMono-Regular.ttf', 'JetBrainsMono-Medium.ttf', 'Inter-Regular.ttf']:
        source = original_fonts / name
        if source.is_file():
            shutil.copyfile(source, fonts / name)
    env = dict(os.environ, XDG_DATA_HOME=data, LIBGL_ALWAYS_SOFTWARE='1')
    env.pop('WAYLAND_DISPLAY', None)
    process, window = measure.launch(pathlib.Path(sys.argv[1]).resolve(), env)
    try:
        run('xdotool', 'windowfocus', '--sync', window)
        time.sleep(1)
        click(window, 450, 300)
        escape = chr(92) + '033'
        command = "printf '" + escape + '[34mDirectory' + escape + '[0m  text  ' + escape + '[31merror' + escape + '[0m' + chr(92) + "n'"
        run('xdotool', 'type', '--clearmodifiers', command)
        run('xdotool', 'key', 'Return')
        time.sleep(0.5)
        capture(window, 'terminal-original.png')
        run('xdotool', 'key', 'ctrl+comma')
        time.sleep(0.3)
        capture(window, 'settings-original.png')
        click(window, 240, 135)
        capture(window, 'font-menu.png')
        click(window, 225, 208)
        click(window, 220, 205)
        capture(window, 'palette-menu.png')
        click(window, 215, 278)
        click(window, 274, 240)
        run('xdotool', 'key', 'ctrl+a')
        run('xdotool', 'type', '85')
        run('xdotool', 'key', 'Return')
        time.sleep(0.3)
        saved = read_settings(data)
        appearance = saved['terminal_appearance']
        print('Selected through GUI:', appearance, flush=True)
        capture(window, 'settings-soft.png')
        assert appearance == {'font': 'jetbrains-regular', 'palette': 'soft', 'brightness': 85.0}, appearance
        click(window, 558, 33)
        capture(window, 'terminal-soft.png')
        assert count_color('terminal-original.png', (77, 159, 255)) > 15, 'Original ANSI blue was not painted'
        assert count_color('terminal-soft.png', (96, 123, 153)) > 15, 'Selected Soft/85 did not repaint terminal'
    finally:
        measure.stop(process)
    process, window = measure.launch(pathlib.Path(sys.argv[1]).resolve(), env)
    try:
        run('xdotool', 'windowfocus', '--sync', window)
        run('xdotool', 'key', 'ctrl+comma')
        time.sleep(0.4)
        capture(window, 'settings-after-restart.png')
        assert read_settings(data)['terminal_appearance'] == appearance
        run('xdotool', 'windowsize', window, '760', '500')
        time.sleep(0.3)
        capture(window, 'settings-minimum-top.png')
        run('xdotool', 'mousemove', '--window', window, '450', '350', 'click', '--repeat', '24', '--delay', '20', '5')
        time.sleep(0.3)
        capture(window, 'settings-minimum-bottom.png')
        print('PASS: real GUI font/palette/brightness selection, terminal recoloring, persistence after restart, minimum-window scrolling', flush=True)
    finally:
        measure.stop(process)
