const { Clutter, Gio, GLib, GObject, St } = imports.gi;
const Main = imports.ui.main;
const PanelMenu = imports.ui.panelMenu;
const PopupMenu = imports.ui.popupMenu;
const ByteArray = imports.byteArray;

const SNAPSHOT_PATH = GLib.build_filenamev([
    GLib.get_user_data_dir(),
    'runwayclock',
    'widget.json',
]);

// Tauri uses the application identifier as the Linux desktop-entry name.
// Keep fallbacks for distro packaging and development installs.
const DESKTOP_APP_IDS = [
    'app.runwayclock.desktop.desktop',
    'RunwayClock.desktop',
    'runwayclock-app.desktop',
];

const RunwayIndicator = GObject.registerClass(
class RunwayIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'RunwayClock');
        this._label = new St.Label({
            text: 'RUNWAY —',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this.add_child(this._label);

        this._openItem = new PopupMenu.PopupMenuItem('Open RunwayClock');
        this._openItem.connect('activate', () => this._openDashboard());

        this._runwayItem = new PopupMenu.PopupMenuItem('Waiting for calculation', {
            reactive: false,
        });
        this._freshnessItem = new PopupMenu.PopupMenuItem('', { reactive: false });
        this._scenarioItem = new PopupMenu.PopupMenuItem('', { reactive: false });
        this.menu.addMenuItem(this._openItem);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this.menu.addMenuItem(this._runwayItem);
        this.menu.addMenuItem(this._freshnessItem);
        this.menu.addMenuItem(this._scenarioItem);

        // The runway number is an ambient shortcut to the full dashboard.
        // Right click still exposes the small status menu.
        this.connect('button-press-event', (_actor, event) => {
            if (event.get_button() !== 1)
                return Clutter.EVENT_PROPAGATE;
            this._openDashboard();
            return Clutter.EVENT_STOP;
        });
        this._refresh();
    }

    _openDashboard() {
        this.menu.close();
        for (const desktopId of DESKTOP_APP_IDS) {
            const app = Gio.DesktopAppInfo.new(desktopId);
            if (app) {
                try {
                    app.launch([], null);
                    return;
                } catch (error) {
                    logError(error, `RunwayClock could not launch ${desktopId}`);
                }
            }
        }

        const executable = GLib.find_program_in_path('runwayclock-app');
        if (executable) {
            try {
                Gio.Subprocess.new([executable], Gio.SubprocessFlags.NONE);
                return;
            } catch (error) {
                logError(error, 'RunwayClock could not launch its executable');
            }
        }

        Main.notify(
            'RunwayClock is not installed',
            'Install the desktop app, then click the runway number to open its dashboard.'
        );
    }

    _refresh() {
        try {
            const [ok, bytes] = GLib.file_get_contents(SNAPSHOT_PATH);
            if (!ok)
                throw new Error('snapshot is not readable');
            const snapshot = JSON.parse(ByteArray.toString(bytes));
            const today = utcToday();
            const zeroDate = snapshot.zero_date ? parseDate(snapshot.zero_date) : null;
            if (zeroDate) {
                const duration = calendarDuration(today, zeroDate);
                this._label.text = `RUNWAY ${duration.short}`;
                this._runwayItem.label.text = `Runway: ${duration.long}`;
            } else {
                this._label.text = 'RUNWAY 100y+';
                this._runwayItem.label.text = 'Reserve not reached in calculation horizon';
            }
            const actualDate = parseDate(snapshot.last_actual_data);
            const age = Math.max(0, daysBetween(actualDate, today));
            this._freshnessItem.label.text = age === 0
                ? 'Actual data: today'
                : `Actual data: ${age} day${age === 1 ? '' : 's'} old`;
            this._scenarioItem.label.text = `Scenario: ${snapshot.scenario} · confidence: ${snapshot.confidence}`;
        } catch (error) {
            this._label.text = 'RUNWAY —';
            this._runwayItem.label.text = 'Run runwayclock calculate first';
            this._freshnessItem.label.text = SNAPSHOT_PATH;
            this._scenarioItem.label.text = '';
        }
    }
});

function parseDate(value) {
    const [year, month, day] = value.split('-').map(Number);
    return new Date(Date.UTC(year, month - 1, day));
}

function utcToday() {
    const now = new Date();
    return new Date(Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()));
}

function daysBetween(start, end) {
    return Math.floor((end.getTime() - start.getTime()) / 86400000);
}

function addClampedMonth(date) {
    const year = date.getUTCFullYear();
    const month = date.getUTCMonth();
    const day = date.getUTCDate();
    const lastDay = new Date(Date.UTC(year, month + 2, 0)).getUTCDate();
    return new Date(Date.UTC(year, month + 1, Math.min(day, lastDay)));
}

function calendarDuration(start, end) {
    if (end <= start)
        return { short: '0d', long: '0 days' };
    let cursor = start;
    let months = 0;
    while (true) {
        const next = addClampedMonth(cursor);
        if (next > end)
            break;
        cursor = next;
        months += 1;
    }
    const days = daysBetween(cursor, end);
    const short = months > 0 ? `${months}m ${days}d` : `${days}d`;
    const parts = [];
    if (months > 0)
        parts.push(`${months} month${months === 1 ? '' : 's'}`);
    if (days > 0 || months === 0)
        parts.push(`${days} day${days === 1 ? '' : 's'}`);
    return { short, long: parts.join(' ') };
}

let indicator = null;
let refreshTimer = null;

function init() {}

function enable() {
    indicator = new RunwayIndicator();
    Main.panel.addToStatusArea('runwayclock', indicator);
    refreshTimer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 3600, () => {
        indicator._refresh();
        return GLib.SOURCE_CONTINUE;
    });
}

function disable() {
    if (refreshTimer !== null) {
        GLib.Source.remove(refreshTimer);
        refreshTimer = null;
    }
    if (indicator !== null) {
        indicator.destroy();
        indicator = null;
    }
}
