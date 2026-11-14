# Tiksu Clipper

Tallenna pelin tai ruudun parhaat hetket jälkikäteen. Tiksu Clipper pitää taustalla muistissa viimeiset minuutit. Kun jotain hienoa tapahtuu, painat yhtä näppäintä, ja juuri tapahtunut tallentuu videoksi.

**[⬇ Lataa Tiksu Clipper (TiksuClipper-Setup.exe)](https://github.com/Pastorilaine/tiksu-clipper/releases/latest/download/TiksuClipper-Setup.exe)**

Ilmainen, ei mainoksia, ei tiliä eikä kirjautumista. Toimii Windows 10:ssä ja 11:ssä.

---

## Mitä sillä voi tehdä

- **Tallenna jälkikäteen.** Paina **F10**, niin viimeiset 2 minuuttia (pituuden voi vaihtaa) tallentuvat MP4-videoksi. Toimii myös koko näytön peleissä.
- **Jaa heti.** Kopioi leike yhdellä napilla ja liitä se **Ctrl+V**:llä Discordiin, WhatsAppiin tai mihin tahansa.
- **Leikkaa.** Valitse leikkeestä paras kohta. Alkuperäinen video säilyy.
- **Ääni mukaan.** Pelin ja tietokoneen äänet ja halutessa oma mikrofonisi.
- **Ei hidasta peliä.** Video pakataan näytönohjaimella. Heikommalle koneelle on oma kevyt laatuasetus.

---

## Asennus

Asennus vie noin minuutin. Järjestelmänvalvojan oikeuksia ei tarvita.

### 1. Lataa asennusohjelma

Lataa **[TiksuClipper-Setup.exe](https://github.com/Pastorilaine/tiksu-clipper/releases/latest/download/TiksuClipper-Setup.exe)**. Linkki lataa aina uusimman version.

Jos selain kysyy, haluatko varmasti säilyttää tiedoston, valitse **Säilytä**.

### 2. Käynnistä asennusohjelma

Avaa ladattu `TiksuClipper-Setup.exe` (yleensä Lataukset-kansiossa).

Windows saattaa näyttää sinisen ikkunan **"Windows suojasi tietokonettasi"**. Se näkyy, koska Tiksu Clipper on uusi, pieni ohjelma, jota Windows ei vielä tunne. Jatka näin:

1. Valitse **Lisätietoja**.
2. Valitse **Suorita silti**.

### 3. Käy asennus läpi

1. Asennusohjelma ehdottaa kansiota. Oletus käy hyvin, joten valitse **Seuraava**.
2. Valitse halutessasi **Luo kuvake työpöydälle** ja sitten **Seuraava**.
3. Valitse **Asenna**.
4. Jätä **Käynnistä Tiksu Clipper** valituksi ja valitse **Valmis**.

Tiksu Clipper löytyy jatkossa Käynnistä-valikosta nimellä **Tiksu Clipper**.

### 4. Ensimmäinen käynnistys

Ensimmäisellä kerralla Tiksu Clipper lataa tarvitsemansa videotyökalun (ffmpeg, noin 35 Mt). Lataus näkyy Yleiskatsaus-sivulla ja kestää yleensä alle minuutin. Ohjelma kuvaa jo latauksen aikana, mutta leikkeen voi tallentaa vasta, kun lataus on valmis. Tämä tehdään vain kerran.

---

## Ensimmäinen leike

1. Kun Tiksu Clipper on auki, yläkulmassa lukee **Puskuri käynnissä**. Se tallentaa nyt taustalla.
2. Tee mitä tahansa: pelaa, katso videota tai selaa.
3. Paina **F10**. Ruudun kulmaan tulee ilmoitus **Leike tallennettu**, ja kuulet merkkiäänen.
4. Avaa **Leikkeet**-sivu. Siellä voit toistaa, leikata, kopioida ja poistaa leikkeitä.

Leikkeet tallentuvat kansioon **Videot\Clips**. Leikkeen nimessä on pelin nimi ja aika, esimerkiksi `cs2_2026-11-14_18-32-05.mp4`.

Kun suljet ikkunan, Tiksu Clipper jää toimimaan taustalle kellon viereen (ilmaisinalue). Ohjelman saa kokonaan kiinni oikean alakulman kuvakkeesta.

---

## Asetukset lyhyesti

Kaikki asetukset ovat ohjelman vasemmassa sivupalkissa.

| Sivu | Mitä siellä säädetään |
|---|---|
| **Kuvauslähde** | Koko näyttö tai yksittäinen ikkuna |
| **Äänet** | Tietokoneen ääni, yksittäinen ohjelma ja mikrofoni |
| **Laatu** | Valmiit laatuprofiilit, leikkeen pituus, kuvakoko ja kuvanopeus. Heikolle koneelle valitse **Heikko kone**. |
| **Pikanäppäimet** | Vaihda F10 toiseen näppäimeen tai lisää näppäimiä eri pituisille leikkeille |
| **Peliprofiilit** | Pelikohtaiset asetukset, jotka vaihtuvat itsestään, kun peli käynnistyy |
| **Sovellus** | Käynnistys Windowsin mukana, ilmoitukset ja päivitykset |
| **Ohje** | Vastauksia yleisimpiin kysymyksiin |

---

## Päivitykset

Tiksu Clipper päivittää itsensä taustalla. Uusi versio otetaan käyttöön, kun ohjelma käynnistyy seuraavan kerran, tai heti **Käynnistä uudelleen** -napista. Sinun ei tarvitse ladata uutta asennusohjelmaa.

---

## Ongelmia?

**Leike ei tallennu.** Katso Yleiskatsaus-sivun kohta **Vaatii huomiota**. Yleisimmät syyt:
- Videotyökalu latautuu vielä (vain ensimmäisellä kerralla).
- Puskuri on pysäytetty. Paina yläkulmasta **Käynnistä puskuri**.
- F10 on toisen ohjelman käytössä, esimerkiksi näytönohjaimen overlayn. Vaihda näppäin Pikanäppäimet-sivulla.

**Kuva on musta tai peli ei näy.** Valitse Kuvauslähde-sivulta **koko näyttö** ja pelaa reunattomassa ikkunatilassa (borderless).

**Leikkeestä puuttuu ääni.** Tarkista Äänet-sivulta, että oikea äänilaite on valittu. Jos haluat oman äänesi mukaan, kytke mikrofoni päälle.

**Kone hidastuu.** Valitse Laatu-sivulta **Heikko kone** -profiili tai pienempi kuvakoko ja 30 FPS.

**Jokin muu.** Sovellus-sivulla on **Avaa loki** -nappi. Liitä loki, kun kerrot ongelmasta [GitHubissa](https://github.com/Pastorilaine/tiksu-clipper/issues).

---

## Poistaminen

Avaa **Asetukset → Sovellukset → Asennetut sovellukset**, etsi **Tiksu Clipper** ja valitse **Poista**.

Leikkeesi jäävät kansioon Videot\Clips. Asetukset jäävät kansioon `%APPDATA%\SillyBorg`. Voit poistaa ne käsin, jos et tarvitse niitä.

---

## Lisenssi

Tiksu Clipper on avointa lähdekoodia [MIT-lisenssillä](LICENSE).

Kolmannen osapuolen osat:
- Rust-kirjastot (`Cargo.lock`) ovat pääosin MIT- ja Apache-2.0-lisensoituja.
- `assets/fonts`: fontit SIL Open Font License -lisenssillä (`assets/fonts/OFL.txt`).
- **FFmpeg** ei kuulu tähän ohjelmistoon. Sovellus lataa ja ajaa sen erillisenä ohjelmana ([Gyan.dev-build](https://github.com/GyanD/codexffmpeg/releases), GPL). Tiksu Clipper ei sisällytä sitä itseensä eikä linkitä siihen.

**Kehittäjille:** kääntäminen, rakenne, testit ja julkaisu ovat tiedostossa [docs/KEHITTAJILLE.md](docs/KEHITTAJILLE.md).
