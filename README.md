# OpenAudioPower

Studio do nagrywania, edycji, przetwarzania i separacji dźwięku dla Windows (Rust + Tauri 2 + Svelte 5).

## Funkcje
- **Wielościeżkowa oś czasu**: klipy, przycinanie, przenoszenie między ścieżkami, narastanie i wygaszanie, dzielenie, kopiowanie, duplikowanie, znaczniki, pętla, siatka (sekundy / takty), cofanie i ponawianie.
- **Nagrywanie** z dowolnego wejścia (WASAPI), metronom, podgląd poziomu na żywo.
- **Import**: MP3, FLAC, WAV, OGG/Vorbis, M4A/AAC, ALAC, AIFF, CAF, MKV/WebM.
- **Separacja ścieżek**: HT-Demucs (6 ścieżek: wokal, perkusja, bas, gitara, pianino, reszta albo 4 ścieżki) przez ONNX Runtime, lokalnie. Modele (136–166 MB) są pobierane na żądanie. Jest też szybki tryb DSP, który nie wymaga pobierania.
- **Efekty w czasie rzeczywistym** na ścieżkach i masterze: korektor parametryczny, tilt EQ, filtry (w tym analogowy ladder, auto-filtr / wah, formantowy, grzebieniowy), kompresor, kompresor trzypasmowy, limiter, clipper, bramka, de-esser, kształtowanie transjentów, pogłos, delay, poszerzenie Haasa, chorus, flanger, phaser, tremolo, vibrato, modulator pierścieniowy, saturacja, exciter, symulacja taśmy, bit crusher, narzędzie stereo, gotowe presety.
- **Przetwarzanie**: normalizacja (szczyt / LUFS / RMS), wzmocnienie, redukcja szumu (z profilem), usuwanie trzasków, odbudowa przesterów, usuwanie przydźwięku sieci, tłumienie głosek wybuchowych, wyrównanie głośności (leveler), zmiana wysokości, rozciąganie w czasie, prędkość, odwracanie, usuwanie składowej stałej, mono/stereo i inne.
- **Analiza**: EBU R128 (LUFS, LRA, true peak), RMS, korelacja, tempo (BPM), tonacja, spektrogram, analizator widma na żywo.
- **Eksport**: WAV 16/24/32f, FLAC 16/24, MP3 (LAME), M4A/AAC (koder Windows), Ogg Vorbis i Opus; cały miks, zaznaczenie, pętla albo każda ścieżka osobno; normalizacja do streamingu, podcastu lub broadcastu.
- **Automatyzacja** głośności i panoramy na każdej ścieżce: punkty rysowane na osi czasu, działają przy odtwarzaniu i eksporcie.
- **Czysta mowa (AI)**: DeepFilterNet3 wbudowany w aplikację (bez pobierania, na procesorze) usuwa szum tła z nagrań głosu, a tłumienie późnego pogłosu zmniejsza echo pomieszczenia.
- **Edytor spektralny** (Ctrl+Shift+R): zaznaczasz na spektrogramie kaszel, dzwonek albo pisk i wypełniasz to miejsce tłem albo tylko je wyciszasz.
- **Remiks** (Ctrl+Shift+M): karaoke jednym kliknięciem (Ctrl+Shift+K), a cappella, osobne suwaki wokalu, perkusji, basu, gitary, pianina i reszty, zmiana tonacji samego podkładu i zgranie remiksu do nowej ścieżki.
- **Wtyczki VST3 i CLAP** jako efekty na ścieżkach i masterze: skanowanie folderów, własne okna wtyczek, stan zapisywany w projekcie. Każda wtyczka działa w osobnym procesie (`oap-plughost.exe`), więc jej awaria albo zawieszenie nie zamyka programu – slot przepuszcza wtedy dźwięk bez zmian i można go uruchomić ponownie.
- **Przetwarzanie wsadowe** (Ctrl+B): ten sam łańcuch operacji i efektów na wielu plikach albo całym folderze, z konwersją formatu.
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
