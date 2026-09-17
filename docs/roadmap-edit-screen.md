# Fotografie-app — Edit Screen Roadmap

## Doel

De edit screen moet doorgroeien van een eenvoudige Fuji RAW-editor naar een volwaardige professionele RAW-editor die kan concurreren met Lightroom CC en Capture One.

Belangrijk uitgangspunt:

> **Simple by default → professional when needed.**

De interface moet rustig en toegankelijk blijven voor beginners, terwijl professionals uiteindelijk volledige controle krijgen.

---

# 1. Aanbevolen edit screen

```text
EDIT

PROFILE
  ▾ Fuji Film Simulation
  ▾ RAW Profile

WHITE BALANCE
  Temperature
  Tint
  Eyedropper

LIGHT
  Exposure
  Contrast
  Highlights
  Shadows
  Whites
  Blacks
  HDR

CURVE
  RGB
  Red
  Green
  Blue
  Luma

COLOUR
  Vibrance
  Saturation
  Color Mixer / HSL
  Point Color

COLOR GRADING
  Shadows
  Midtones
  Highlights
  Global
  Blending
  Balance

EFFECTS
  Texture
  Clarity
  Dehaze
  Vignette
  Grain

DETAIL
  Sharpening
  Noise Reduction
  AI Denoise
  Moiré
  Defringe

OPTICS
  Lens Corrections
  Chromatic Aberration
  Distortion
  Vignetting

GEOMETRY
  Rotate
  Straighten
  Perspective
  Vertical
  Horizontal
  Aspect

MASKING
  Brush
  Linear Gradient
  Radial Gradient
  Select Subject
  Select Sky
  Select Background
  Color Range
  Luminance Range

RETOUCH
  Heal
  Clone
  Remove

LENS BLUR
  Depth Blur
  Focus
  Bokeh

CALIBRATION
  Red Primary
  Green Primary
  Blue Primary
```

Niet alle onderdelen hoeven standaard geopend te zijn. Gebruik collapsible sections om de interface rustig te houden.

---

# 2. Histogram

Het histogram moet permanent of eenvoudig bereikbaar zichtbaar zijn.

```text
RGB HISTOGRAM
▁▂▃▄▅▆▇▆▅▄▃▂▁
```

Mogelijkheden:

- RGB histogram
- R / G / B kanalen
- Shadow clipping indicator
- Highlight clipping indicator
- Clipping preview in de foto

Het histogram is belangrijk voor een serieuze RAW-workflow.

---

# 3. Curves

Een volledige curve-editor is een must-have.

### Kanalen

- RGB
- Red
- Green
- Blue
- Luma

### Functionaliteit

- Point curve
- Histogram achter de curve
- Black point
- White point
- Auto curve
- Reset
- Lineaire curve

De curve moet direct en vloeiend reageren en geschikt zijn voor professionele kleurcorrectie.

---

# 4. Colour / Color Mixer

De huidige Colour-sectie moet worden uitgebreid.

## Basis

- Vibrance
- Saturation

## HSL / Color Mixer

Kleuren:

- Red
- Orange
- Yellow
- Green
- Aqua
- Blue
- Purple
- Magenta

Per kleur:

- Hue
- Saturation
- Luminance

---

# 5. Point Color

Point Color is een belangrijke professionele feature.

Workflow:

1. Gebruiker activeert Point Color.
2. Gebruiker klikt op een kleur in de foto.
3. De app detecteert de geselecteerde kleur.
4. Gebruiker kan Hue, Saturation en Luminance aanpassen.

Bijvoorbeeld:

```text
Point Color — Red

Hue          -8
Saturation  +14
Luminance     +5
```

Idealiter wordt ook het geselecteerde kleurgebied gevisualiseerd.

---

# 6. Color Grading

Los van HSL.

Gebruik afzonderlijke controls voor:

- Shadows
- Midtones
- Highlights
- Global

Extra:

- Hue
- Saturation
- Luminance
- Blending
- Balance

Een interface met drie kleurwielen voor Shadows / Midtones / Highlights zou hier goed passen.

---

# 7. Effects

De huidige Effects-sectie kan worden uitgebreid met:

- Texture
- Clarity
- Dehaze
- Vignette
- Grain

Voor Vignette eventueel:

- Amount
- Midpoint
- Roundness
- Feather
- Highlights

Voor Grain:

- Amount
- Size
- Roughness

---

# 8. Detail

Een complete professionele detailsectie.

## Sharpening

- Amount
- Radius
- Detail
- Masking

## Noise Reduction

- Luminance
- Detail
- Contrast
- Color
- Color Detail

## AI Denoise

```text
AI DENOISE

Amount ─────────●────

[ Enhance ]
```

AI Denoise is belangrijk voor moderne RAW-workflows, maar hoeft niet de eerste prioriteit te zijn.

## Overige

- Moiré
- Defringe

---

# 9. Optics

Automatische lenscorrecties moeten onderdeel worden van de RAW pipeline.

```text
OPTICS

☑ Enable Lens Correction
☑ Remove Chromatic Aberration

Lens Profile
  Fujifilm
  XF 16-55mm F2.8
  ...

Distortion
Vignetting
Defringe
```

De app moet camera- en lensmetadata uitlezen en waar mogelijk automatisch het juiste lensprofiel toepassen.

Voorbeeld:

```text
FUJIFILM X-T5
XF 16-55mm F2.8
16mm
f/2.8
```

---

# 10. Fuji Film Simulations

Dit is een belangrijke potentiële USP van de app.

Onder Profile:

```text
PROFILE

FUJIFILM
────────────────

PROVIA
Velvia
ASTIA
Classic Chrome
Classic Neg
Nostalgic Neg
Acros
Eterna
Eterna Bleach Bypass
Sepia
...
```

Belangrijk technisch onderscheid:

### Profile ≠ Preset

Een Film Simulation / RAW Profile bepaalt de rendering van de RAW.

Een Preset bevat vervolgens edit-instellingen zoals Exposure, Contrast, Color Grading, enzovoort.

---

# 11. Masking

Masking is waarschijnlijk één van de belangrijkste ontbrekende professionele features.

## Handmatige masks

- Brush
- Linear Gradient
- Radial Gradient

## Selecties

- Color Range
- Luminance Range

## AI masks

- Subject
- Sky
- Background
- People

---

# 12. Masks als Layers

Masks moeten non-destructief werken.

Voorbeeld:

```text
MASKS

┌──────────────────────┐
│ 👤 Eagle             │
│    Exposure +0.2     │
│                      │
│ ☁ Sky                │
│    Highlights -30    │
│                      │
│ ○ Background         │
└──────────────────────┘
```

Per mask:

- Visibility
- Opacity
- Rename
- Duplicate
- Invert
- Delete
- Add / Subtract
- Mask overlay

Een mask moet zelf een verzameling van adjustments kunnen bevatten.

Bijvoorbeeld:

```text
Mask — Sky

Exposure      -0.4
Highlights    -30
Dehaze        +10
Temperature    -5
```

---

# 13. Geometry

Voor perspectief en compositie:

```text
GEOMETRY

Rotate
Straighten

Perspective
  Vertical
  Horizontal
  Rotate
  Aspect

Auto
Guided
```

Bij Guided Perspective kan de gebruiker lijnen over architectuur trekken waarna de app de perspectiefcorrectie berekent.

---

# 14. Crop

Crop liever als aparte tool dan als gewone sidebar-sectie.

Toolbar:

```text
[ Crop ] [ Straighten ] [ Mask ] [ Heal ]
```

Aspect ratio's:

- Free
- Original
- 1:1
- 4:5
- 3:2
- 16:9

Extra:

- Rotate
- Flip Horizontal
- Flip Vertical
- Straighten
- Aspect lock

---

# 15. Retouch

Minimaal:

```text
RETOUCH

Heal
Clone
Remove
```

Brush controls:

- Size
- Feather
- Opacity
- Source

Focus eerst op een extreem goede Heal + Clone workflow voordat complexere generative AI wordt toegevoegd.

---

# 16. Lens Blur

Een latere feature.

```text
LENS BLUR

Depth
     ─────●────

Focus
     ────●─────

Blur
     ───────●──

Bokeh
  Natural
  Circular
  ...
```

Mogelijk gebaseerd op een depth map.

Dit is interessant als onderscheidende / high-end feature, maar geen Tier 1-prioriteit.

---

# 17. Calibration

Een professionele geavanceerde sectie.

```text
CALIBRATION

Red Primary
  Hue
  Saturation

Green Primary
  Hue
  Saturation

Blue Primary
  Hue
  Saturation
```

Niet noodzakelijk voor beginners, maar interessant voor professionele gebruikers en color workflows.

---

# 18. Before / After

Een snelle before/after workflow is essentieel.

Toolbar:

```text
↶   ↷   |   BEFORE
```

Mogelijkheden:

- Before / After toggle
- Voorbeeld met ingedrukte Space
- Keyboard shortcut
- Split view (optioneel)

Bijvoorbeeld:

```text
Space = Before
Y = Before / After
```

---

# 19. History

Een volledige edit history.

```text
HISTORY

Exposure +0.4
Highlights -20
Curve
Color
Mask — Sky
Film Simulation
...
```

Elke wijziging moet non-destructief opgeslagen worden.

De gebruiker moet eenvoudig terug kunnen naar een eerder punt.

---

# 20. Copy / Paste Adjustments

Zeer belangrijk voor fotografieworkflows.

Selecteer meerdere foto's en gebruik:

**Copy Adjustments**

Daarna:

**Paste Adjustments**

Maar geef controle over wat wordt gekopieerd:

```text
PASTE ADJUSTMENTS

☑ White Balance
☑ Exposure
☑ Color
☑ Curve
☐ Crop
☑ Masks
☑ Effects
☑ Detail
☑ Optics
```

Dit maakt batch editing veel krachtiger.

---

# 21. Presets

Voeg uiteindelijk een goede preset-workflow toe.

Mogelijkheden:

- Create Preset
- Save Preset
- Apply Preset
- Import Preset
- Export Preset
- User presets
- Factory presets

Ook belangrijk:

```text
Preset

☑ Light
☑ Color
☑ Effects
☑ Detail
☐ White Balance
☐ Crop
```

Zo kan een preset alleen bepaalde onderdelen beïnvloeden.

---

# 22. Recommended Edit Screen IA

Een goede eindstructuur:

```text
┌─────────────────────────────┐
│ HISTOGRAM                   │
│ ▂▃▄▆▇▆▄▃▂                   │
├─────────────────────────────┤
│ PROFILE                  ›  │
│ WHITE BALANCE            ›  │
│ LIGHT                    ›  │
│ CURVE                    ›  │
│ COLOUR                   ›  │
│ COLOR GRADING             › │
│ EFFECTS                  ›  │
│ DETAIL                   ›  │
│ OPTICS                   ›  │
│ GEOMETRY                ›  │
│ MASKS                   ›  │
│ RETOUCH                 ›  │
│ LENS BLUR               ›  │
│ CALIBRATION             ›  │
└─────────────────────────────┘
```

Niet alles standaard openen.

De gebruiker moet eerst de belangrijkste controls zien en pas bij `Advanced` dieper kunnen gaan.

---

# 23. Prioriteiten voor development

## 🔴 Tier 1 — Must have

1. Histogram
2. Curves
3. HSL / Color Mixer
4. Color Grading
5. Masks + Layers
6. Brush mask
7. Linear Gradient
8. Radial Gradient
9. Crop + Straighten
10. Lens Correction
11. Sharpening
12. Noise Reduction
13. Heal / Clone
14. Before / After
15. History
16. Copy / Paste Adjustments
17. Fuji Film Simulations

---

## 🟠 Tier 2 — Professional parity

18. AI Denoise
19. Subject Mask
20. Sky Mask
21. Background Mask
22. Color Range Mask
23. Luminance Range Mask
24. Geometry / Keystone
25. Defringe
26. Moiré
27. Grain
28. Calibration
29. Presets
30. Profiles
31. Batch Editing

---

## 🟡 Tier 3 — Advanced / Differentiating

32. Lens Blur / Depth Map
33. AI Remove
34. People Masking
35. Face / Eye Retouching
36. Smart Auto Edit
37. Reference View
38. Snapshots / Versions
39. AI Suggested Edits

---

# 24. Product philosophy

De app moet niet proberen Lightroom simpelweg na te bouwen.

Een sterkere positionering:

> **A native, extremely fast desktop RAW editor for Linux, with excellent Fujifilm support.**

De UX moet daarom draaien om:

### Simple by default

Beginners zien alleen de belangrijkste controls.

### Professional when needed

Professionals kunnen iedere laag van de image pipeline openen.

### Non-destructive

RAW en edits blijven gescheiden.

### Contextual

Tools verschijnen waar ze nodig zijn, zonder onnodige modals of schermen.

### Fast

De editor moet direct reageren op iedere wijziging.

### Fuji-first

Fujifilm RAW rendering en Film Simulations moeten een belangrijke kwaliteits- en productdifferentiator worden.

---

# 25. Belangrijkste UX-principe

De belangrijkste ontwerpregel voor het edit-scherm:

> **Maak het niet eenvoudiger door functionaliteit weg te halen. Maak het eenvoudiger door functionaliteit goed te verbergen totdat de gebruiker het nodig heeft.**

Dus:

```text
LIGHT
Exposure
Contrast
Highlights
Shadows
Whites
Blacks

Advanced ▾
  Curve
  HDR
```

en:

```text
COLOUR
Vibrance
Saturation

Advanced ▾
  HSL
  Point Color
  Color Grading
```

Hierdoor blijft de interface rustig, terwijl de app uiteindelijk dezelfde professionele diepgang kan bieden als Lightroom of Capture One.
