# Paralelizacija Blockchain Consensus Algoritama: Proof-of-Work i Proof-of-Stake


### Radim za ocenu 10 ###

## Opis problema

Implementacija i analiza paralelnih algoritama za dva različita blockchain consensus mehanizma, Proof-of-Work (PoW) i Proof-of-Stake (PoS). PoW predstavlja compute-intensive problem idealan za HPC analizu, dok PoS simulira distribuiranu validaciju transakcija. Projekat omogućava poređenje dva fundamentalno različita pristupa postizanja konsenzusa u blockchain sistemima analizom paralelnih performansi i teorijskih karakteristika.

## 1. Proof-of-Work Implementacija

### 1.1. Problem i pristup

PoW consensus zahteva pronalaženje nonce vrednosti koja, kada se hešuje zajedno sa sadržajem bloka, proizvodi hash koji zadovoljava određeni difficulty (broj vodećih nula). Ovaj proces je paralelizabilan jer različiti thread-ovi/procesi mogu nezavisno testirati različite nonce vrednosti.

### 1.2. Python implementacija

*   **Sekvencijalna verzija:** Mining proces koji sekvencijalno testira nonce vrednosti redom dok se ne pronađe validna vrednost. Implementacija uključuje kreiranje blockchain strukture sa genesis blokom i linkovanjem blokova preko `previous hash` polja.
*   **Paralelizovana verzija:** Pool mining arhitektura gde višestruki worker procesi testiraju različite opsege nonce vrednosti simultano. Implementacija load balancing mehanizma koji omogućava workerima da preuzimaju nove chunk-ove posla kada završe sa dodeljenim opsegom, kako bi se izbeglo idle vreme.

**Izlazni podaci:**

*   Mining progress po iteracijama
*   Finalna blockchain struktura
*   Performance metrike (ukupno vreme, hash rate, broj testiranih nonce-a)

### 1.3. Rust implementacija

*   **Sekvencijalna verzija:** Optimizovana implementacija fokusirana na memory efficiency i brzinu hash kalkulacija.
*   **Paralelizovana verzija:** Thread-based paralelizacija sa automatskim mehanizmom za distribuciju posla. Implementacija thread-safe koordinacije kroz atomske operacije za praćenje globalnog stanja i channel-based komunikacije za razmenu rezultata.

**Izlazni podaci:**

*   Mining progress po iteracijama
*   Finalna blockchain struktura
*   Performance metrike (ukupno vreme, hash rate, broj testiranih nonce-a)

### 1.4. Eksperimenti skaliranja

*   **Jako skaliranje:** Fiksiranje veličine problema (konstantan nivo težine) i merenje kako se vreme izvršavanja menja sa povećanjem broja thread-ova/procesa. Svaka konfiguracija se izvršava 30 puta za statističku relevantnost.
    *   Cilj je merenje speedup faktora i efikasnosti paralelizacije, kao i identifikacija tačke nakon koje dodavanje više thread-ova ne donosi značajno poboljšanje.
*   **Slabo skaliranje:** Povećavanje veličine problema (težine) proporcionalno broju thread-ova, tako da svaki thread ima konstantan obim posla.
    *   Idealno vreme izvršavanja treba da ostane konstantno ako je sistem perfektno skalabilan.

**Analiza:**

*   Izračunavanje speedup faktora
*   Efikasnost
*   Procena sekvencijalnog dela koda koji se ne može paralelizovati
*   Teorijski maksimum speedup-a po Amdalovom zakonu
*   Scaled speedup po Gustafsonovom zakonu za slabo skaliranje
*   Poređenje Python vs Rust performance karakteristika

**Izlazni podaci:** Tabele sa rezultatima za svaku konfiguraciju koja sadrže:

*   Srednje vreme izvršavanja
*   Standardnu devijaciju
*   Min/Max vrednosti
*   Identifikovane outlier-e sa objašnjenjem
*   95% interval poverenja

### 1.5. Vizualizacija

Rust aplikacija koja učitava generisane podatke i kreira grafike pomoću biblioteke za plotting.

**Grafici:**

*   Jako skaliranje za Python (speedup vs broj thread-ova, sa Amdahlovom teorijskom linijom)
*   Jako skaliranje za Rust (speedup vs broj thread-ova, sa Amdahlovom teorijskom linijom)
*   Slabo skaliranje za Python (scaled speedup vs broj thread-ova, sa Gustafsonovom teorijskom linijom)
*   Slabo skaliranje za Rust (scaled speedup vs broj thread-ova, sa Gustafsonovom teorijskom linijom)


## 2. Proof-of-Stake Implementacija

### 2.1. Problem i pristup

PoS consensus ne zahteva compute-intensive mining, već koristi stake-based odabir validatora. Međutim, može se simulirati scenario gde više validatora paralelno validira transakcije unutar bloka, pri čemu prvi koji uspešno validira, objavljuje blok.


### 2.2. Rust implementacija

*   **Multi-validator arhitektura:** Kreiranje više thread-ova koji predstavljaju nezavisne validatore, svaki sa određenim ulogom (stake). Validatori konkurentno validiraju isti blok, pri čemu svaki validator nezavisno verifikuje transakcije. Thread-safe koordinacija se postiže kroz atomske operacije za praćenje globalnog stanja validacije i channel-based komunikaciju za razmenu rezultata.

*   **Konkurentna validacija blokova:** Validatori paralelno izvršavaju proces validacije bloka. Prvi validator koji uspešno završi celokupnu validaciju objavljuje blok i signalizira ostalim validatorima da prekinu validaciju.

*   **Opciono: Byzantine Fault Tolerance (BFT):** Umesto "prvi završi" modela, može se implementirati BFT konsenzus gde validatori šalju ateste (potvrde) nakon validacije. Blok se smatra finalizovanim kada prikupi 2/3+ atesta od ukupnog broja validatora, što omogućava toleranciju do 1/3 malicioznih ili neispravnih validatora. Ovaj pristup bolje odražava realne PoS sisteme kao što su Tendermint ili Ethereum 2.0 Casper FFG.

**Metrike:**

*   Vreme validacije bloka po validatoru
*   Distribucija pobeda po validatorima (uticaj stake-a)
*   Propusnost (transakcije po sekundi)

**Izlazni podaci:**

*   Log validacionih aktivnosti
*   PoS blockchain struktura

## 3. Izveštaj i analiza

### 3.1. Tehnička specifikacija sistema

Opis hardverskog i softverskog okruženja na kom su eksperimenti izvršeni:

*   Procesor (model, broj jezgara, takt, organizacija keša)
*   RAM (tip, količina)
*   Operativni sistem
*   Verzije programskih jezika i biblioteka

### 3.2. Analiza sekvencijalnog i paralelnog dela

*   **Za PoW:** Procena koliki procenat koda je sekvencijalan (inicijalizacija, finalizacija, koordinacija) i koliki je paralelizabilan (petlja za testiranje nonce-a). Na osnovu ove analize, izračunavanje teorijskog maksimuma speedup-a po Amdalovom zakonu.
*   **Za PoS:** Analiza koordinacionog overhead-a kod odabira validatora i distribucije validacije transakcija.

### 3.3. Teorijsko poređenje PoW vs PoS

Istraživanje literature i rezimiranje ključnih razlika između dva algoritma:

*   **Energetska potrošnja:**
    *   PoW: ekstremno visoka energija zbog konstantnog računanja heša
    *   PoS: minimalna potrošnja, samo digitalna verifikacija
    *   Konkretni primeri iz literature (Bitcoin vs Ethereum)
*   **Sigurnost i vektori napada:**
    *   PoW: 51% hash rate napad
    *   PoS: 51% stake napad, nothing-at-stake problem, long-range attacks
    *   Trade-off-ovi između sigurnosti i efikasnosti
*   **Decentralizacija:**
    *   PoW: rizik centralizacije u mining poolovima
    *   PoS: rizik plutokratije (koncentracija uloga)
*   **Performanse:**
    *   PoW throughput ograničen težinom mininga
    *   PoS brže kreiranje bloka i finalnost
    *   Latencija do finalnosti transakcija



## Proširenje za diplomski rad

## 5. P2P Blockchain Mreža

### 5.1. Distribuirana arhitektura

Implementacija decentralizovane P2P mreže gde svaki čvor održava svoju kopiju blockchain-a i učestvuje u consensus procesu.

*   **Bootstrap node:** Inicijalni čvor koji pokreće mrežu i omogućava otkrivanje peer-ova. Svi regularni čvorovi se konektuju na bootstrap čvor koji prosleđuje poruke.
*   **Regular nodes:** Čvorovi koji se priključuju mreži, sinhronizuju blockchain i učestvuju u mining-u (PoW).

### 5.2. P2P komunikacija

Definisanje protokola za komunikaciju između čvorova preko TCP socketa. Tipovi poruka uključuju:

*   Join/discovery poruke
*   Block broadcasting
*   Blockchain sinhronizacija
*   Mining koordinacija (start/stop signali)
*   Heartbeat za održavanje konekcija

### 5.3. PoW distribuirani mining

*   **Mining koordinacija:** Bootstrap čvor koordinira mining proces slanjem start signala sa šablonom bloka. Svi čvorovi počinju mining istog bloka paralelno. Prvi koji pronađe validnu nonce vrednost, objavljuje blok svim peer-ovima. Ostali validiraju primljeni blok i ako je validan, prekidaju svoj mining i prelaze na sledeći blok.

### 5.4. Konsenzus mehanizam

*   **Pravilo najdužeg lanca (Longest chain rule):** Kada čvor primi novi blok, poredi dužinu svog lanca sa novim lancem. Ako je novi lanac duži i svi blokovi su validni, čvor prihvata novi lanac.
*   **Validacija bloka:**
    *   Validnost previous hash linka
    *   Validnost proof-of-work (hash zadovoljava difficulty)


### 5.5. CLI Interfejs

Interfejs za pokretanje čvorova u različitim režimima:

*   **Bootstrap mode** (prvi čvor u mreži)
*   **Regular mode** (konektovanje na postojeću mrežu)
*   **Runtime komande** za mining, prikaz blockchain-a, liste peer-ova

### 5.6. Mrežno logovanje i metrike


*   Log mrežnih događaja (sve P2P poruke)
*   Log koordinacije mininga (start/stop događaji, ko je našao koji blok)
