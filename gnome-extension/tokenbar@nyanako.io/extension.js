// TokenBar GNOME Shell extension.
//
// A top-bar button (cat icon + today's cost) with a popover showing today's
// usage and subscription-quota. It reads the cache the TokenBar GTK app writes
// (~/.cache/tokenbar-gtk/{graph,quota}-cache.json) — the Shell can't host the
// app's GTK4/OpenGL UI, so the full 3D graph and lenses open in the GTK app via
// "Open TokenBar". "Refresh" runs `tokenbar-gtk --refresh` to update the cache.

import GObject from 'gi://GObject';
import St from 'gi://St';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';

import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const CACHE_DIR = GLib.build_filenamev([GLib.get_user_cache_dir(), 'tokenbar-gtk']);
const REFRESH_SECS = 30;

function readCache(name) {
    try {
        const path = GLib.build_filenamev([CACHE_DIR, name]);
        const [ok, bytes] = GLib.file_get_contents(path);
        if (!ok) return null;
        return JSON.parse(new TextDecoder().decode(bytes))?.payload ?? null;
    } catch (_e) {
        return null;
    }
}

function compact(n) {
    const a = Math.abs(n);
    if (a >= 1e9) return (n / 1e9).toFixed(1) + 'B';
    if (a >= 1e6) return (n / 1e6).toFixed(1) + 'M';
    if (a >= 1e3) return (n / 1e3).toFixed(1) + 'K';
    return String(Math.round(n));
}

function mostRecentDay(graph) {
    const days = graph?.contributions;
    if (!Array.isArray(days) || days.length === 0) return null;
    return days.reduce((best, d) => (best && best.date > d.date ? best : d), null);
}

const TokenBarButton = GObject.registerClass(
class TokenBarButton extends PanelMenu.Button {
    _init(extension) {
        super._init(0.0, 'TokenBar');
        this._extension = extension;

        const box = new St.BoxLayout({ style_class: 'tokenbar-panel-box' });
        box.add_child(new St.Icon({
            gicon: Gio.icon_new_for_string(GLib.build_filenamev([extension.path, 'cat.png'])),
            style_class: 'system-status-icon',
        }));
        this._label = new St.Label({
            text: '…',
            y_align: Clutter.ActorAlign.CENTER,
            style_class: 'tokenbar-panel-label',
        });
        box.add_child(this._label);
        this.add_child(box);

        this._buildMenu();
        this._refresh();
        this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, REFRESH_SECS, () => {
            this._refresh();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _buildMenu() {
        this._summary = new PopupMenu.PopupMenuItem('', { reactive: false });
        this.menu.addMenuItem(this._summary);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._quota = new PopupMenu.PopupMenuSection();
        this.menu.addMenuItem(this._quota);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        const refresh = new PopupMenu.PopupMenuItem('Refresh');
        refresh.connect('activate', () => this._spawn(['tokenbar-gtk', '--refresh'], () => this._refresh()));
        this.menu.addMenuItem(refresh);

        const open = new PopupMenu.PopupMenuItem('Open TokenBar');
        // Force GLX: GTK4 defaults to EGL, which fails to create a GL context on
        // this NVIDIA/X11 setup ("Unable to create GL context"). GLX is reliable
        // here. The binary self-forces this too (force_glx_on_x11), but older
        // installed builds predate that, so set it at the launch boundary.
        open.connect('activate', () => this._spawn(['tokenbar-gtk'], null, { TOKENBAR_NO_TRAY: '1', GDK_DEBUG: 'gl-glx' }));
        this.menu.addMenuItem(open);

        // Refresh the cache view whenever the popover opens.
        this.menu.connect('open-state-changed', (_m, open) => {
            if (open) this._refresh();
        });
    }

    _refresh() {
        const today = mostRecentDay(readCache('graph-cache.json'));
        if (today) {
            this._label.text = '$' + Math.round(today.totals.cost);
            this._summary.label.text =
                `Today: ${compact(today.totals.tokens)} tokens · $${today.totals.cost.toFixed(2)}`;
        } else {
            this._label.text = '—';
            this._summary.label.text = 'No data yet — use “Refresh” or open TokenBar';
        }

        this._quota.removeAll();
        const agents = readCache('quota-cache.json')?.agents;
        if (Array.isArray(agents)) {
            for (const ag of agents) {
                const name = (ag.clientId ?? 'agent');
                const nice = name.charAt(0).toUpperCase() + name.slice(1);
                if (ag.error) {
                    this._quota.addMenuItem(new PopupMenu.PopupMenuItem(`${nice}: not available`, { reactive: false }));
                    continue;
                }
                for (const w of (ag.windows ?? [])) {
                    const pct = Math.round(w.remainingPercent ?? 0);
                    const reset = w.resetText ? ` · ${w.resetText}` : '';
                    this._quota.addMenuItem(
                        new PopupMenu.PopupMenuItem(`${nice} · ${w.label}: ${pct}%${reset}`, { reactive: false }));
                }
            }
        }
    }

    _spawn(argv, onDone, extraEnv) {
        try {
            const launcher = new Gio.SubprocessLauncher({ flags: Gio.SubprocessFlags.NONE });
            if (extraEnv) {
                for (const key in extraEnv) launcher.setenv(key, extraEnv[key], true);
            }
            const proc = launcher.spawnv(argv);
            if (onDone) proc.wait_async(null, () => onDone());
        } catch (e) {
            logError(e, 'TokenBar: failed to spawn ' + argv.join(' '));
        }
    }

    destroy() {
        if (this._timer) {
            GLib.source_remove(this._timer);
            this._timer = null;
        }
        super.destroy();
    }
});

export default class TokenBarExtension extends Extension {
    enable() {
        this._button = new TokenBarButton(this);
        Main.panel.addToStatusArea(this.uuid, this._button);
    }

    disable() {
        this._button?.destroy();
        this._button = null;
    }
}
