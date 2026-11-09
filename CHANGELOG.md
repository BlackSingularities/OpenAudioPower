# Changelog

## 1.2.0

- 13 nowych efektów w czasie rzeczywistym. Filtry: analogowy ladder (dolno-, pasmowo- i górnoprzepustowy z rezonansem i nasyceniem), automatyczny / wah (LFO albo obwiednia), formantowy z płynną zmianą samogłosek, tilt EQ, grzebieniowy. Dynamika: kompresor trzypasmowy, kształtowanie transjentów, clipper. Barwa: exciter, symulacja taśmy (wow, flutter, szum). Modulacja: modulator pierścieniowy, vibrato. Przestrzeń: poszerzenie Haasa.
- Nowe operacje naprawcze w panelu Obróbka i w przetwarzaniu wsadowym: odbudowa przesterowanych szczytów, usuwanie przydźwięku sieci 50/60 Hz z harmonicznymi, tłumienie głosek wybuchowych, wyrównanie głośności (leveler).
- Menu „Dodaj efekt” jest pogrupowane w podmenu według kategorii.

## 1.1.0

- Eksport do MP3, M4A (AAC), OGG Vorbis i Opus z wyborem jakości – z okna eksportu, eksportu ścieżek i z biblioteki.
- Automatyzacja głośności i panoramy: przycisk z wykresem w nagłówku ścieżki pokazuje krzywą; kliknięcie dodaje punkt, przeciąganie go przesuwa (Shift – tylko wartość), dwuklik usuwa.
- Przetwarzanie wsadowe (Narzędzia → Przetwarzanie wsadowe, Ctrl+B): pliki albo cały folder, dowolny łańcuch kroków z panelu Obróbka i efektów, zapis obok oryginałów lub do wybranego folderu.
- Czysta mowa (AI) w panelu Obróbka i w przetwarzaniu wsadowym: DeepFilterNet3 usuwa szum tła z nagrań głosu, opcjonalnie razem z pogłosem pomieszczenia. Osobno: usuwanie pogłosu.
- Edytor spektralny (Narzędzia, menu klipu, Ctrl+Shift+R): zaznaczenie na spektrogramie wypełnia się tłem sprzed i zza niego albo wycisza o wybraną liczbę dB; podgląd odsłuchu zaznaczenia.
- Remiks (Narzędzia, menu klipu, Ctrl+Shift+M): rozdzielenie utworu jednym przyciskiem, karaoke, a cappella, suwaki każdej ścieżki na żywo, tonacja podkładu ±6 półtonów (opcjonalnie z wokalem), zgranie do nowej ścieżki. Karaoke od razu: Ctrl+Shift+K.
- Wtyczki VST3 i CLAP (efekty): Efekty → Dodaj → Wtyczka VST3 / CLAP. Skanowanie standardowych folderów i własnych, wyszukiwarka, okno wtyczki, stan wtyczki zapisywany w projekcie .oap. Każda wtyczka działa w osobnym procesie – gdy się wywróci albo zawiesi, program działa dalej, dźwięk przechodzi bez zmian, a wtyczkę można uruchomić ponownie z ostatnim stanem. Instrumenty (syntezatory) jeszcze nie są obsługiwane.
- Okno eksportu nie ucina już przycisków.

## 1.0.0

Pierwsza wersja.

- Wielościeżkowa oś czasu: klipy, przycinanie, dzielenie, narastanie i wygaszanie, znaczniki, pętla, siatka sekund lub taktów, cofanie i ponawianie.
- Nagrywanie z dowolnego wejścia (WASAPI) z metronomem i miernikiem poziomu.
- Separacja na wokal, perkusję, bas, gitarę, pianino i resztę (HT-Demucs). Na karcie graficznej (DirectML) utwór rozdziela się kilkanaście razy szybciej, niż trwa; bez zgodnej karty liczy procesor. Jest też szybki tryb DSP bez pobierania modelu.
- Efekty w czasie rzeczywistym na ścieżkach i masterze, przetwarzanie (normalizacja LUFS, odszumianie z profilem, usuwanie trzasków, zmiana wysokości i tempa) oraz analiza (LUFS, true peak, BPM, tonacja, spektrogram).
- Eksport WAV i FLAC: miks, zaznaczenie, pętla albo każda ścieżka osobno.
- Zgłaszanie błędów i sugestii oraz automatyczne aktualizacje przez redzikowski.com.
