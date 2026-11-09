# Changelog

## 1.5.0

- Korektor dynamiczny: pasmo (dzwon albo półka), które tnie lub podbija tylko wtedy, gdy jego zakres przekroczy próg – np. na syczące „s” albo dudniący bas.
- Korektor parametryczny: każde pasmo może działać na całość, tylko na środek (M) albo tylko na boki (S), a tryb liniowej fazy przenosi całą krzywą do filtra FIR bez przesunięć fazy (opóźnienie jest kompensowane).
- Limiter true-peak: wykrywa szczyty między próbkami na sygnale nadpróbkowanym 4×, więc sufit dotyczy dBTP. Domyślnie włączony w nowych limiterach, w presecie masteringu i przy normalizacji eksportu.
- Imager stereo w pasmach: osobna szerokość basu, środka i góry (np. bas w mono, góra szersza), zwrotnice Linkwitza-Rileya bez zmiany barwy.
- Match EQ (Narzędzia, menu klipu): dopasowuje barwę klipu do nagrania wzorcowego z biblioteki filtrem liniowej fazy, z podglądem krzywej i regulacją ilości.

## 1.4.0

- Skracanie pauz wewnątrz nagrania (jak Truncate Silence w Audacity): pauzy cichsze od progu i dłuższe od zadanej długości są skracane, a miejsca cięcia łączy krótki crossfade.
- Auto Duck (menu ścieżki, Ścieżki → Auto Duck): ścisza ścieżkę tam, gdzie gra inna, i zapisuje to jako krzywą głośności, którą można dalej poprawiać.
- Paulstretch: ekstremalne, „ambientowe” rozciągnięcie dźwięku nawet 50-krotnie.
- Płynna zmiana tempa i wysokości od początku do końca zaznaczenia (Sliding Stretch).
- Powtarzanie fragmentu oraz zmiana częstotliwości próbkowania: przeliczenie z ograniczeniem pasma (np. dźwięk telefoniczny 8 kHz) albo odtwarzanie jak nagrane w innej częstotliwości.

## 1.3.0

Mikser:
- Szyny: ścieżka może grać na szynę zamiast na master, a wysyłki (przed albo za suwakiem) dokładają jej sygnał do dowolnej szyny – np. jeden pogłos dla kilku ścieżek („Wysyłka na nowy pogłos”). Pętle sygnału są blokowane.
- Sidechain w kompresorze: kompresor reaguje na inną ścieżkę, np. muzyka ścisza się, gdy mówi lektor. Ścieżka-klucz może być wyciszona.
- Grupy ścieżek: wspólny suwak (o tyle samo dB), wyciszenie i solo.
- Solo podąża za routingiem: solo ścieżki zostawia szyny, do których gra, a solo szyny – ścieżki, które ją zasilają.
- Kompensacja opóźnień wtyczek (np. look-ahead limitera) także przy odtwarzaniu, z uwzględnieniem szyn i wysyłek.
- Zamrażanie ścieżki: klipy z efektami są renderowane do jednego nagrania, które gra zamiast nich i nie obciąża procesora.

Edycja:
- Nachodzące na siebie klipy na jednej ścieżce przechodzą jeden w drugi (crossfade o stałej mocy) zamiast się sumować.
- Ołówek (P): rysowanie pojedynczych próbek po przybliżeniu osi czasu.
- Generator sygnału (Ctrl+Shift+G): ton, przemiatanie, szum biały / różowy / brązowy, DTMF, ścieżka klików, cisza.
- Etykiety: punkty i zakresy z opisem (Ctrl+L, lista Ctrl+Shift+L), import i eksport plików etykiet Audacity, eksport wielu plików według etykiet.
- Historia zmian (Ctrl+H) z nazwami kroków; kliknięcie kroku przenosi do stanu po nim.
- Punch-in: nagrywanie tylko w zakresie pętli, z kilkusekundowym odtwarzaniem wstępnym i samoczynnym końcem. Nagrywanie z timerem: start o godzinie albo za N minut, przez określony czas.

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
