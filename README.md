# libinput-scroll-hook (Rust, LD_PRELOAD)

LD_PRELOAD-Hook, der libinput-Scrollwerte unter GNOME/Wayland skaliert.

- Touchpad (FINGER/CONTINUOUS) wird skaliert.
- Mausrad (WHEEL/v120) bleibt unverändert, außer `LIBINPUT_SCROLL_SCALE_WHEEL` ist gesetzt.

## Build (Linux)

Voraussetzungen (Ubuntu 24.04+/25.04):

```bash
sudo apt update
sudo apt install -y build-essential pkg-config
# Rust installieren, falls noch nicht vorhanden
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Bauen (Release)
cargo build --release -p libinput_scroll_hook
```

Ergebnis: `target/release/liblibinput_scroll_hook.so`

## Nutzung

Temporär (für Test in einer App):

```bash
export LIBINPUT_SCROLL_SCALE=1.6              # globaler Faktor (x & y)
export LIBINPUT_SCROLL_SCALE_X=1.4            # optional: horizontal
export LIBINPUT_SCROLL_SCALE_Y=1.8            # optional: vertikal
export LIBINPUT_SCROLL_DEBUG=1                # optional: Logging
# export LIBINPUT_SCROLL_SCALE_WHEEL=1.0      # optional: Mausrad skalieren (Standard: aus)

LD_PRELOAD=/pfad/zu/liblibinput_scroll_hook.so <programm>
```

Systemweit (GNOME/Wayland, user-scope):

Variante A: `~/.config/environment.d/99-libinput-scroll.conf`

```ini
# ~/.config/environment.d/99-libinput-scroll.conf
LD_PRELOAD=/home/<user>/pfad/zu/target/release/liblibinput_scroll_hook.so
LIBINPUT_SCROLL_SCALE=1.6
# LIBINPUT_SCROLL_SCALE_X=1.4
# LIBINPUT_SCROLL_SCALE_Y=1.8
# LIBINPUT_SCROLL_DEBUG=1
```

Danach ab- und wieder anmelden.

Variante B (Fallback): systemd-User-Override für `gnome-shell.service` oder `gnome-session`. Achtung: je nach Distro/Version kann das abweichen.

```bash
systemctl --user edit gnome-shell.service
# Einfügen (oder analog für gnome-session):
[Service]
Environment=LD_PRELOAD=/home/<user>/.../liblibinput_scroll_hook.so
Environment=LIBINPUT_SCROLL_SCALE=1.6
```

## Funktionsweise

Das `.so` überschreibt folgende libinput-Symbole und leitet intern auf die Originalfunktionen weiter (via `dlsym(RTLD_NEXT, ...)`):

- `libinput_event_pointer_get_axis_value(event, axis)`
  - Skaliert nur bei `axis_source` = FINGER oder CONTINUOUS.
- `libinput_event_pointer_get_axis_value_discrete(event, axis)`
  - Unverändert (nur Logging). Diskrete Steps (Mausrad) sollen unverändert bleiben.
- `libinput_event_pointer_get_scroll_value_v120(event, axis)`
  - Unverändert, außer `LIBINPUT_SCROLL_SCALE_WHEEL` ist gesetzt.

Alle FFI-Typen werden minimal als Opaque-Typen abgebildet, die Enums sind per `u32` und dokumentierten Werten hinterlegt.

## Hinweise

- Getestet werden sollte auf Ubuntu 25.04 mit GNOME/Wayland.
- Falls GNOME/Captive-Services LD_PRELOAD bereinigen, Variante A (environment.d) ist am robustesten.
- Für Debugging `LIBINPUT_SCROLL_DEBUG=1` setzen und Applogs (stderr) prüfen.

## Lizenz

MIT oder Apache-2.0
