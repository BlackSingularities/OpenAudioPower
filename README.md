# OpenAudioPower

Studio do nagrywania, edycji, przetwarzania i separacji dźwięku dla Windows (Rust + Tauri 2 + Svelte 5).

## Funkcje
- **Wielościeżkowa oś czasu**: klipy, przycinanie, przenoszenie między ścieżkami, narastanie i wygaszanie, dzielenie, kopiowanie, duplikowanie, znaczniki, pętla, siatka (sekundy / takty), cofanie i ponawianie.
- **Nagrywanie** z dowolnego wejścia (WASAPI), metronom, podgląd poziomu na żywo.
- **Import**: MP3, FLAC, WAV, OGG/Vorbis, M4A/AAC, ALAC, AIFF, CAF, MKV/WebM.
- **Separacja ścieżek**: HT-Demucs (6 ścieżek: wokal, perkusja, bas, gitara, pianino, reszta albo 4 ścieżki) przez ONNX Runtime, lokalnie. Modele (136–166 MB) są pobierane na żądanie. Jest też szybki tryb DSP, który nie wymaga pobierania.
- **Efekty w czasie rzeczywistym** na ścieżkach i masterze: korektor parametryczny, filtry, kompresor, limiter, bramka, de-esser, pogłos, delay, chorus, flanger, phaser, tremolo, saturacja, bit crusher, narzędzie stereo, gotowe presety.
- **Przetwarzanie**: normalizacja (szczyt / LUFS / RMS), wzmocnienie, redukcja szumu (z profilem), usuwanie trzasków, zmiana wysokości, rozciąganie w czasie, prędkość, odwracanie, usuwanie składowej stałej, mono/stereo i inne.
- **Analiza**: EBU R128 (LUFS, LRA, true peak), RMS, korelacja, tempo (BPM), tonacja, spektrogram, analizator widma na żywo.
- **Eksport**: WAV 16/24/32f, FLAC 16/24; cały miks, zaznaczenie, pętla albo każda ścieżka osobno; normalizacja do streamingu, podcastu lub broadcastu.
- **Projekty `.oap`**: JSON plus folder z audio w FLAC.
- Zgłaszanie błędów i sugestii oraz sprawdzanie aktualizacji przez redzikowski.com (tak samo jak w pozostałych aplikacjach).

## Budowanie
```
npm install
npx tauri dev      # tryb deweloperski
npx tauri build    # instalator NSIS
```
Wymagany toolchain MSVC (`rust-toolchain.toml`). ONNX Runtime jest linkowany statycznie, a `DirectML.dll` trafia do instalatora.

Autor: [redzikowski.com](https://redzikowski.com)
