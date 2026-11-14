# Tiksu Clipper kehittäjille

Käyttäjän ohjeet ovat [README:ssä](../README.md). Tämä sivu kertoo, miten sovellus on tehty, käännetään ja julkaistaan.

## Tekniikka lyhyesti

- Rust 2024, käyttöliittymä egui/eframe.
- Kuvankaappaus Windows Graphics Capturella ja H.264-koodaus Media Foundationin (MediaTranscoder) kautta. Windows valitsee NVENC-, AMF- tai Quick Sync -enkooderin itse.
- Puskuri on rengas 10 sekunnin H.264-segmenttejä muistissa tai levyllä. Ääni (WASAPI: laite, yksittäinen sovellus, mikrofoni) on samalla QPC-kellolla omissa renkaissaan.
- Tallennettaessa ffmpeg yhdistää segmentit ilman uudelleenkoodausta (stream copy), lisää AAC-ääniraidat ja BT.709-värimerkinnät.
- High-profiili. Bittinopeus skaalautuu kuvakoon mukaan: `bitrate · (pikselit / 1080p)^0.8`, rajattu 2–150 Mbps. Kuvan voi pienentää (`max_height`).
- MediaTranscoder ei anna säätää B-kehyksiä, GOP-pituutta tai rate controlia, joten laatua säädetään bittinopeudella, kuvakoolla ja kuvanopeudella.
- `vendor/windows-capture` on pieni paikattu kopio (kuvan pienennys ja profiili), muutokset `vendor/windows-capture/PATCHES.md`.
- Trimmaus koodaa uudelleen järjestyksessä `h264_nvenc`, `h264_amf`, `h264_qsv`, `libx264` ja lopuksi stream copy.
- ffmpeg ja ffprobe ajetaan matalalla prioriteetilla. Jos ne puuttuvat, sovellus lataa kerran kiinnitetyn version (noin 35 Mt, SHA-256-tarkistus) kansioon `%LOCALAPPDATA%\TiksuClipper\ffmpeg`. `ffmpeg.exe` exen vieressä tai PATHissa käytetään sellaisenaan.

### Kuorma

Mitattu RTX 4060 -koneella, 3440×1440 @ 60 FPS, 40 Mbps, 30 s leike, sovelluksen ääni ja mikrofoni päällä:

| | |
|---|---|
| exe | 9,4 Mt |
| RAM (working set) | ~230–290 Mt, josta videopuskuri noin 200 Mt |
| CPU | ~4–6 % yhdestä ytimestä rauhallisella ruudulla, enemmän nopeassa liikkeessä |
| Levylle kirjoitus | ~0 Mt/min (vain tallennettaessa) |

Muistipuskurin koko on suunnilleen bittinopeus / 8 × (pisin leike + 10 s).

## Kääntäminen ja ajaminen

```powershell
cargo build --release
.\target\release\silly-borg.exe          # ikkuna auki
.\target\release\silly-borg.exe --tray   # suoraan ilmaisinalueelle
```

- Asetukset: `%APPDATA%\SillyBorg\config.json`
- Loki: `%APPDATA%\SillyBorg\tiksu.log` (`println!` ja `eprintln!` ohjautuvat sinne)
- Debug-käännöksen voi avata tiettyyn sivuun: `TIKSU_PAGE=Laatu`

## Testit

```powershell
cargo test                                      # yksikkötestit
cargo test end_to_end -- --ignored              # oikea kaappaus + 3 ääniraitaa
cargo test real_clip -- --ignored               # trimmaus/pikkukuva edellisen leikkeelle
cargo test clipboard -- --ignored               # leike leikepöydälle (ylikirjoittaa leikepöydän)
```

`end_to_end` kuvaa oikeasti näyttöä ja ääntä. Asetuksia voi kokeilla ympäristömuuttujilla `TIKSU_E2E_FPS`, `TIKSU_E2E_MBPS`, `TIKSU_E2E_SECS` ja `TIKSU_E2E_HEIGHT`. CI ajaa clippyn, yksikkötestit ja riippuvuuksien haavoittuvuustarkistuksen jokaisesta pushista.

## Päivitykset

Sovellus tarkistaa päivitykset 20 s käynnistyksen jälkeen ja sen jälkeen 6 tunnin välein.

1. Se lukee `latest.json`:n kahdesta lähteestä ja valitsee uudemman version:
   - `https://github.com/Pastorilaine/tiksu-clipper/releases/latest/download/latest.json`
   - `https://tiksu.fi/clipper/latest.json`
2. Se lataa exen ja tarkistaa sen **ed25519-allekirjoituksen**. Allekirjoitus kattaa sekä version että exen tavut, joten muokattua tai vanhaa exeä ei asenneta, vaikka lähde olisi murrettu.
3. Se vaihtaa exen pyörivän ohjelman alta. Uusi versio käynnistyy seuraavalla kerralla tai heti Käynnistä uudelleen -napista.

Jos exe on kansiossa, johon ei voi kirjoittaa (esim. `Program Files`), sovellus ohjaa lataussivulle.

## Julkaisu

**Ennen tagia, savutesti puhtaalla koneella** (Windows Sandbox riittää; GitHubin ajurissa ei ole näytönohjainta, joten CI ei kaappaa):

1. Asenna `TiksuClipper-Setup.exe` (käännös `ISCC.exe /DAppVersion=x.y.z installer\TiksuClipper.iss`). ffmpeg latautuu ensikäynnistyksellä.
2. Tallenna leike pikanäppäimellä. Leike aukeaa ja siinä on ääni.
3. Leikkaa leike ja kopioi se leikepöydälle. Ctrl+V liittää tiedoston kansioon.
4. Päivitys edellisestä versiosta: asenna vanha, aseta `TIKSU_UPDATE_SOURCE` uuteen `latest.json`:iin ja paina Tarkista nyt.

```powershell
# 1. Nosta version Cargo.toml:ssa, esim. 0.4.2, ja commitoi
git tag -a v0.4.2 -m "Mitä uutta tässä versiossa"   # viesti = päivityksen muutoslista
git push origin main v0.4.2
```

`.github/workflows/release.yml` ajaa testit, kääntää, allekirjoittaa ja luo GitHub-julkaisun, jossa on `TiksuClipper.exe` (päivityksille), `TiksuClipper-Setup.exe` (asennusohjelma, Inno Setup -skripti `installer/TiksuClipper.iss`) ja `latest.json`. Cargo.toml:n version pitää olla sama kuin tagin. Työnkulku tarvitsee repoon secretin `TIKSU_SIGNING_KEY`, jonka arvo on avaintiedoston sisältö.

Asennusohjelma asentaa ilman admin-oikeuksia kansioon `%LOCALAPPDATA%\Programs\TiksuClipper`, johon sovellus voi itse kirjoittaa päivityksen.

### tiksu.fi

- **Latauslinkki**, joka osoittaa aina uusimpaan versioon: `https://github.com/Pastorilaine/tiksu-clipper/releases/latest/download/TiksuClipper-Setup.exe`
- **Päivityslähde:** kopioi julkaisun `latest.json` osoitteeseen `https://tiksu.fi/clipper/latest.json`. Jos haluat jakaa myös exen tiksu.fi:stä, kopioi se ja vaihda `latest.json`:n `url`. Allekirjoitus pysyy voimassa, koska se ei koske URL:ia.

### Allekirjoitusavain

- Avain luodaan komennolla `cargo run --release --bin tiksu-release -- keygen <tiedosto>`. Nykyinen avain on tiedostossa `%USERPROFILE%\.tiksu\clipper-signing.key`, ja sen julkinen puoli on `src/update.rs`:n `PUBLIC_KEY`.
- **Ota avaimesta varmuuskopio.** Jos avain katoaa, asennetut sovellukset eivät enää hyväksy päivityksiä, ja käyttäjien täytyy ladata uusi versio käsin.
- Avain ei koskaan päädy repoon (`*.key` on `.gitignore`:ssa).
- Käsin allekirjoitus: `tiksu-release sign --exe TiksuClipper.exe --version 0.4.2 --url <url> --key <avain> --out latest.json`
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
- `src/win.rs`: Win32-apurit: yksi instanssi, näytönohjain, prosessit, automaattinen käynnistys, roskakori, leikepöytä
- `src/applog.rs`: lokitiedosto
- `src/ui/mod.rs`: ikkuna, sivupalkki, yläpalkki, ilmoitukset ja sivurekisteri (`Page`); `src/ui/pages/`: yksi tiedosto per sivu
- `src/tiksu.rs`: käyttöliittymän tyylijärjestelmä (värit, fontit, napit, paneelit, rivit), pohjana IT-Veljekset DESIGN.md
- `src/config.rs`, `src/devices.rs`, `src/notification.rs`: asetukset, laitteet ja ikkunat, merkkiäänet

### Uuden sivun lisääminen

1. Lisää `Page`-enumiin (`src/ui/mod.rs`) uusi vaihtoehto ja täytä se kohtiin `ALL`, `label`, `icon` ja `group`. Sivupalkki ja murupolku poimivat sen taulukosta.
2. Lisää rivi `render_page`-funktioon.
3. Kirjoita `src/ui/pages/<nimi>.rs`: yksi `impl SillyBorgApp` -metodi, joka kutsuu `tk::page_head(...)` ja `tk::content(...)` ja rakentaa sisällön paneeleista (`tk::panel`) ja riveistä (`tk::row`, `tk::row_wide`, `tk::toggle`). Listaa tiedosto tiedostossa `src/ui/pages/mod.rs`.
4. Kääntäjä huomauttaa jokaisesta `match`-lausekkeesta, johon uusi sivu puuttuu.

Ohje-sivun kysymykset ovat datana tiedostossa `src/ui/pages/help.rs` (`FAQ`).
