// GNOME 42 to 44, which load extensions the older way; otherwise the same as ../modern.
const {Gio} = imports.gi;

const INTERFACE = `
<node>
  <interface name="org.gnome.Shell.Extensions.Kairo">
    <method name="Focused">
      <arg type="u" direction="out" name="pid"/>
      <arg type="s" direction="out" name="title"/>
      <arg type="t" direction="out" name="id"/>
      <arg type="b" direction="out" name="fullscreen"/>
    </method>
    <method name="Minimize">
      <arg type="t" direction="in" name="id"/>
      <arg type="b" direction="out" name="minimized"/>
    </method>
  </interface>
</node>`;

class KairoExtension {
    enable() {
        this._service = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this);
        this._service.export(Gio.DBus.session, '/org/gnome/Shell/Extensions/Kairo');
    }

    disable() {
        this._service.unexport();
        this._service = null;
    }

    Focused() {
        const window = global.display.focus_window;
        if (!window)
            return [0, '', 0, false];
        return [Math.max(window.get_pid(), 0), window.get_title() || '', window.get_id(), window.is_fullscreen()];
    }

    Minimize(id) {
        const window = global.get_window_actors().map(a => a.meta_window).find(w => w.get_id() === id);
        if (!window || !window.can_minimize())
            return false;
        window.minimize();
        return true;
    }
}

function init() {
    return new KairoExtension();
}
