# Annota

Herramienta ligera de capturas de pantalla con anotaciones (estilo Lightshot) para Linux,
priorizando GNOME + Wayland.

Flujo: ejecutar `annota` → la pantalla se congela y oscurece → arrastrar para seleccionar
una región → anotar con la barra flotante → copiar (Ctrl+C) o guardar (Ctrl+S).

## Compilar y ejecutar

```sh
sudo apt install build-essential pkg-config libgtk-4-dev   # Ubuntu / Debian
sudo dnf install gcc pkgconf-pkg-config gtk4-devel          # Fedora
cargo run --release
```

Requisitos: Rust ≥ 1.92, GTK ≥ 4.14 (Ubuntu 24.04, Fedora 40+, Debian 13), xdg-desktop-portal.

## Permisos

GNOME guarda el permiso de captura por aplicación y lo pide **una sola vez** con un diálogo
del sistema, que solo puede mostrar la aplicación que tiene el foco. Como Annota captura antes
de abrir ventanas, la primera vez muestra una ventana «Conceder permiso»: al pulsarla, GNOME
pregunta y el permiso queda guardado.

- El portal identifica la aplicación por su *scope* de systemd. Lanzado desde el menú o un
  atajo es `io.github.annota.Annota` (requiere tenerlo instalado). Desde una terminal normal
  no hay ID y GNOME lo permite. Desde la terminal de un IDE empaquetado (snap/flatpak, p. ej.
  RustRover) la captura se atribuye al IDE y falla: usa una terminal normal.
- Si denegaste el permiso, restablécelo con:

  ```sh
  gdbus call --session --dest org.freedesktop.impl.portal.PermissionStore \
    --object-path /org/freedesktop/impl/portal/PermissionStore \
    --method org.freedesktop.impl.portal.PermissionStore.DeletePermission \
    screenshot screenshot io.github.annota.Annota
  ```

## Atajo de teclado (Print Screen)

Wayland no permite a una aplicación capturar teclas globales; se configura en GNOME:

1. Configuración → Teclado → Ver y personalizar atajos → Capturas de pantalla:
   quita la tecla `Impr Pant` de «Hacer una captura de pantalla interactiva».
2. Atajos personalizados → `+` → Nombre `Annota`, Comando `annota`, Atajo `Impr Pant`.

El lanzador del menú usa `annota --delay 500` para que la vista de Actividades termine de
cerrarse antes de capturar; el atajo de teclado no necesita retraso.

Si Annota ya está en ejecución (por ejemplo, sirviendo el portapapeles), el atajo
reutiliza el mismo proceso.

## Uso

| Acción | Cómo |
|---|---|
| Seleccionar región | Arrastrar. Las asas del borde la redimensionan después. |
| Dibujar | Elegir herramienta y arrastrar dentro/fuera de la región (se recorta a la región). |
| Seleccionar / mover anotación | Clic sobre ella (en rectángulos, sobre el borde) y arrastrar. |
| Redimensionar | Asas de la anotación seleccionada (rectángulo, pixelado, extremos de línea/flecha). |
| Cambiar color / grosor / estilo | Con la anotación seleccionada, usar la barra. |
| Texto | Herramienta Texto, clic y escribir. Enter = nueva línea, Esc / Ctrl+Enter / clic fuera = terminar. Clic sobre un texto existente para editarlo. |
| Shift | Líneas y flechas a múltiplos de 45°; rectángulos y pixelado cuadrados. |
| Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y | Deshacer / rehacer |
| Ctrl+B | Negrita (texto) |
| Ctrl+Shift+> / Ctrl+Shift+< | Aumentar / reducir tamaño de texto (también botones A+ / A− y la lista de tamaños) |
| Delete | Borrar anotación seleccionada |
| Ctrl+C | Copiar al portapapeles y cerrar |
| Ctrl+S | Guardar PNG y cerrar |
| Esc | Terminar texto / cerrar popover / cancelar |

La configuración (herramienta, color, grosor, estilo, tamaño de texto, carpeta de guardado)
se guarda en `~/.config/annota/config.toml`.

## Arquitectura

```
src/
├── main.rs          punto de entrada
├── app.rs           ciclo de vida de gtk::Application (instancia única por D-Bus)
├── capture/         trait ScreenshotBackend + portal.rs (xdg-desktop-portal vía ashpd)
├── editor/          modelo puro: anotaciones (un módulo por tipo), geometría, hit-testing,
│                    historial (Command pattern), Canvas (máquina de estados), render Cairo.
│                    Sin GTK → portable y testeable.
├── ui/              overlay.rs (sesión: una ventana por monitor), view.rs (widget de dibujo),
│                    toolbar.rs (barra flotante)
├── clipboard/       copia de la imagen final (gdk::Clipboard) y mantener vivo el proceso
├── export/          rasterizado captura + anotaciones → PNG (Cairo)
└── config/          preferencias (serde + toml)
```

Principios:

- **Las anotaciones se guardan en píxeles de la captura**, nunca en coordenadas lógicas de GTK.
  Cada vista convierte: `píxel = origen_del_monitor + lógico × escala`. Los grosores elegidos
  en la barra son lógicos y se multiplican por la escala, así se ven igual a 100 % y a 200 %.
- La captura original es inmutable; solo se rasteriza al copiar/guardar.
- El fondo se dibuja como textura (GPU); Cairo solo dibuja las anotaciones dentro de la región.
- Undo/redo: `Command::{Add, Remove, Replace}`. `Replace` cubre mover, redimensionar, color,
  grosor, estilo y texto. Los arrastres del control de grosor se agrupan en un solo paso.
- Para añadir Windows: implementar `ScreenshotBackend` (p. ej. Windows Graphics Capture) que
  devuelva un `gdk::Texture`; editor, UI y exportación no cambian.

## Limitaciones conocidas

- **Captura**: en Wayland se usa el portal `org.freedesktop.portal.Screenshot`. La primera
  vez hace falta conceder permiso (ver «Permisos»). El portal de GNOME escribe un archivo (`~/Pictures/Screenshot*.png`);
  Annota lo borra tras cargarlo.
- **Monitores con escalas distintas**: GNOME entrega una sola imagen a una única escala;
  Annota asume un factor común (píxeles de captura / píxeles lógicos). Si no cuadra, se
  muestra un aviso en la terminal y las posiciones pueden desplazarse. Con escalas iguales
  (incluidas 125–200 %) funciona correctamente.
- **Selección entre monitores**: se puede arrastrar de un monitor a otro, pero la barra aparece
  en el monitor donde empezó la selección.
- **Ventanas**: Wayland no permite posicionar ventanas; se usa `fullscreen_on_monitor` por monitor.
- **Portapapeles**: en Wayland el contenido lo sirve el proceso que copió. Tras Ctrl+C las
  ventanas se cierran y el proceso sigue vivo (sin ventana) hasta que otra aplicación copie
  algo o pasen 5 minutos; entonces termina y la imagen ya no se puede pegar.
- **Texto**: usa la API de texto simple de Cairo (sin Pango): sin ajuste de línea ni cursor
  movible (se escribe y borra al final). Teclas muertas e IME funcionan vía GtkIMMulticontext.
- **Lápiz y texto** se pueden mover pero no redimensionar.
- **Pixelado** usa solo la captura original: no pixela anotaciones que estén debajo.
- KDE Plasma y X11 deberían funcionar vía portal, pero no se han probado.

## Empaquetado

```sh
cargo install cargo-deb && cargo deb          # → target/debian/annota_*.deb
```

Flatpak (runtime GNOME 51): ver `flatpak/io.github.annota.Annota.yml`; requiere generar
`cargo-sources.json` con `flatpak-cargo-generator.py` (flatpak-builder-tools).
