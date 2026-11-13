# Tiksu Clipper

Kevyt Windows-leiketallennin (instant replay). Pyörii taustalla ja tallentaa pikanäppäimellä viimeiset sekunnit tai minuutit MP4-leikkeeksi.

## Ominaisuudet

- **Puskuri muistissa:** viimeiset minuutit odottavat valmiiksi pakattuina RAM:issa. Levylle kirjoitetaan vasta tallennettaessa. Halutessa puskurin voi pitää myös levyllä.
- **Laitteistoenkoodaus:** Windows Graphics Capture ja H.264 Media Foundationin kautta. Windows valitsee NVENC-, AMF- tai Quick Sync -enkooderin itse.
- **Ääni:** äänilaitteen loopback, yksittäisen sovelluksen äänet ja mikrofoni. Halutessa tietokone ja mikrofoni tallentuvat myös omille raidoilleen.
- **Pikanäppäimet:** yksi pääpikanäppäin ja enintään kolme lisänäppäintä eri pituisille leikkeille (15 s – 5 min). Toimivat myös koko näytön peleissä.
- **Leikkeet-sivu:** pikkukuvat, toisto, kansioon siirtyminen ja poisto roskakoriin. Lisäksi trimmaus: filmiliuska, kaksi kahvaa ja ruudun tarkka leikkaus, ja alkuperäinen leike säilyy.
- **Peliprofiilit:** kun peli tulee etualalle, sen kuvanopeus, bittinopeus, leikkeen pituus ja tarvittaessa pelkän peli-ikkunan kuvaus otetaan käyttöön automaattisesti.
- **Ikkunakaappaus prosessin mukaan:** ikkuna löytyy, vaikka sen otsikko vaihtuisi. Jos ikkunaa ei ole auki, kuvataan näyttöä, ja kuvaus vaihtuu ikkunaan heti, kun se avautuu.
- **Ilmaisinalue:** ikkunan sulkeminen jättää puskurin käyntiin. Sovelluksen voi käynnistää Windowsin mukana suoraan ilmaisinalueelle.
- **Ilmoitukset:** pieni ilmoitus ruudun kulmassa näkyy ikkunoitujen ja reunattomien pelien päällä. Merkkiääni on eri onnistumiselle ja virheelle.
- **Yksi instanssi:** uusi käynnistys tuo jo käynnissä olevan ikkunan esiin.
- **Loki:** `%APPDATA%\SillyBorg\tiksu.log`.

## Kuinka kevyt

Mitattu RTX 4060 -koneella, 3440×1440 @ 60 FPS, 40 Mbps, 30 s leike, sovelluksen ääni ja mikrofoni päällä:

| | |
|---|---|
| exe | 9,4 Mt |
| RAM (working set) | ~230–290 Mt, josta videopuskuri noin 200 Mt |
| CPU | ~4–6 % yhdestä ytimestä rauhallisella ruudulla, enemmän nopeassa liikkeessä |
| Levylle kirjoitus | ~0 Mt/min (vain tallennettaessa) |

Muistipuskurin koko on suunnilleen bittinopeus / 8 × (pisin leike + 10 s). Laatu-sivu näyttää arvion valituilla asetuksilla.

## Vaatimukset

- Windows 10 1903 tai uudempi (mukana tulevat `curl.exe` ja `tar.exe` riittävät, muuta ei asenneta)
- Mitään ei tarvitse ladata erikseen. Jos `ffmpeg` ja `ffprobe` puuttuvat, sovellus lataa kerran valmiin version (noin 35 Mt, SHA-256-tarkistuksella) kansioon `%LOCALAPPDATA%\TiksuClipper\ffmpeg`. Puskuri kuvaa jo latauksen aikana. Jos `ffmpeg.exe` on `silly-borg.exe`:n vieressä tai PATHissa, sitä käytetään sellaisenaan.

## Ajaminen

```powershell
cargo build --release
.\target\release\silly-borg.exe          # ikkuna auki
.\target\release\silly-borg.exe --tray   # suoraan ilmaisinalueelle
```

Asetukset tallentuvat tiedostoon `%APPDATA%\SillyBorg\config.json`, ja niitä muokataan sovelluksen omilla sivuilla.

## Päivitykset ja jakelu

Sovellus tarkistaa päivitykset 20 s käynnistyksen jälkeen ja sen jälkeen 6 tunnin välein. Tarkistuksen voi tehdä myös Sovellus-sivun napista.

1. Se lukee `latest.json`:n kahdesta lähteestä ja valitsee uudemman version:
   - `https://github.com/Pastorilaine/tiksu-clipper/releases/latest/download/latest.json`
   - `https://tiksu.fi/clipper/latest.json`
2. Se lataa exen ja tarkistaa sen **ed25519-allekirjoituksen**. Allekirjoitus kattaa sekä version että exen tavut, joten muokattua tai vanhaa exeä ei asenneta, vaikka lähde olisi murrettu.
3. Se vaihtaa exen pyörivän ohjelman alta. Uusi versio käynnistyy seuraavalla kerralla, tai heti "Käynnistä uudelleen" -napista.

Automaattisen asennuksen voi kytkeä pois päältä, jolloin sovellus vain ilmoittaa uudesta versiosta. Jos exe on kansiossa, johon ei voi kirjoittaa (esim. `Program Files`), sovellus ohjaa lataussivulle.

### Julkaisu (GitHub Actions)

```powershell
# 1. Nosta version Cargo.toml:ssa, esim. 0.3.0, ja commitoi
git tag -a v0.3.0 -m "Mitä uutta tässä versiossa"   # viesti = päivityksen muutoslista
git push origin main v0.3.0
```

`.github/workflows/release.yml` ajaa testit, kääntää, allekirjoittaa ja luo GitHub-julkaisun, jossa on tiedostot `TiksuClipper.exe` ja `latest.json`. Se tarvitsee repoon secretin `TIKSU_SIGNING_KEY`, jonka arvo on avaintiedoston sisältö.

### tiksu.fi

- **Latauslinkki**, joka osoittaa aina uusimpaan versioon: `https://github.com/Pastorilaine/tiksu-clipper/releases/latest/download/TiksuClipper.exe`
- **Päivityslähde:** kopioi julkaisun `latest.json` osoitteeseen `https://tiksu.fi/clipper/latest.json`. Jos haluat jakaa myös exen tiksu.fi:stä, kopioi se ja vaihda `latest.json`:n `url`. Allekirjoitus pysyy voimassa, koska se ei koske URL:ia.

### Allekirjoitusavain

- Avain luodaan komennolla `cargo run --release --bin tiksu-release -- keygen <tiedosto>`. Nykyinen avain on tiedostossa `%USERPROFILE%\.tiksu\clipper-signing.key`, ja sen julkinen puoli on `src/update.rs`:n `PUBLIC_KEY`.
- **Ota avaimesta varmuuskopio.** Jos avain katoaa, asennetut sovellukset eivät enää hyväksy päivityksiä, ja käyttäjien täytyy ladata uusi versio käsin.
- Avain ei koskaan päädy repoon (`*.key` on `.gitignore`:ssa).
- Käsin allekirjoitus: `tiksu-release sign --exe TiksuClipper.exe --version 0.3.0 --url <url> --key <avain> --out latest.json`
- Testaus ilman julkaisua: aseta `TIKSU_UPDATE_SOURCE=<oma latest.json -osoite>`. Allekirjoitus vaaditaan silti, ja testi-instanssi pyörii erillään oikeasta.

## Rakenne

- `src/main.rs`: käynnistys, yksi instanssi, lokimakrot
- `src/capture.rs`: valvoja (käynnistys, profiilit, ikkunan seuranta), WGC-kaappaus, segmentit ja tallennus
- `src/audio.rs`: WASAPI-kaappaus, työpöytä- ja mikrofoniväylät kellosynkronoituina rengaspuskureina
- `src/buffer.rs`: videosegmenttien rengaspuskuri (muisti tai levy)
- `src/muxer.rs`: ffmpeg-ajot: yhdistäminen, trimmaus, pikkukuvat ja kestot
- `src/ffmpeg.rs`: lataa ffmpegin ensikäynnistyksellä (kiinnitetty versio, SHA-256)
- `src/library.rs`: leikekirjasto ja taustatyöntekijä pikkukuville
- `src/hotkey.rs`: globaalit pikanäppäimet
- `src/tray.rs`: ilmaisinalueen kuvake ja valikko
- `src/overlay.rs`: ilmoitus ruudun kulmassa (Win32)
- `src/update.rs`, `src/manifest.rs`: automaattinen päivitys ja allekirjoitukset; `src/bin/tiksu-release.rs`: julkaisun allekirjoitustyökalu
- `src/win.rs`: Win32-apurit: yksi instanssi, näytönohjain, prosessit, automaattinen käynnistys, roskakori
- `src/applog.rs`: lokitiedosto
- `src/ui.rs`, `src/tiksu.rs`: käyttöliittymä ja Tiksu-tyyli
- `src/config.rs`, `src/devices.rs`, `src/notification.rs`: asetukset, laitteet ja ikkunat, merkkiäänet

## Lisenssi

Tiksu Clipper on avointa lähdekoodia [MIT-lisenssillä](LICENSE). Koodia saa käyttää, muokata ja jakaa vapaasti.

Kolmannen osapuolen osat:
- Rust-kirjastot (`Cargo.lock`) ovat pääosin MIT- ja Apache-2.0-lisensoituja.
- `assets/fonts`: fontit SIL Open Font License -lisenssillä (`assets/fonts/OFL.txt`).
- **FFmpeg** ei kuulu tähän ohjelmistoon. Sovellus lataa ja ajaa sen erillisenä ohjelmana ([Gyan.dev-build](https://github.com/GyanD/codexffmpeg/releases), GPL). Tiksu Clipper ei sisällytä sitä itseensä eikä linkitä siihen.

## Testit

```powershell
cargo test                                      # yksikkötestit
cargo test end_to_end -- --ignored              # oikea kaappaus + 3 ääniraitaa
cargo test real_clip -- --ignored               # trimmaus/pikkukuva edellisen leikkeelle
```
