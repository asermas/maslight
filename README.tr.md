<p align="center">
  <img src="assets/brand/banner.svg" alt="MasLight" width="720">
</p>

<p align="center">
  <a href="LICENSE"><img alt="MIT lisansı" src="https://img.shields.io/badge/lisans-MIT-yellow.svg"></a>
  <img alt="Platformlar" src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-0078D6">
  <img alt="Rust" src="https://img.shields.io/badge/gelistirildi-Rust-000000?logo=rust">
  <img alt="Telemetri yok" src="https://img.shields.io/badge/telemetri-yok-16a34a">
</p>

**English: [README.md](README.md)**

MasLight, ekranındaki görüntüden adreslenebilir LED şeridini sürer. WLED UDP,
DDP, sACN, Art-Net ve Adalight konuşur; Windows ve Linux'ta çalışır; her şey
kendi makinende kalır.

> **Durum: 0.2.0 geliştiriliyor.** Windows yolu tamamlandı ve gerçek donanımda
> doğrulandı: yakalama, renk hattı, ses, script efektleri ve UDP çıkışı uçtan
> uca çalışıyor. Linux X11 arka ucu CI'da Xvfb altında gerçekten çalıştırılıyor
> ama henüz fiziksel bir şerit sürmedi. Wayland yarım; hangi yarısının bittiğini
> [Linux sayfası](docs/linux.md) açıkça anlatıyor.

## Neden var

Bu iş için çoğu kişinin kullandığı Prismatik bugün fiilen Windows-only:
Linux derlemesi X11'e bağlı ve Wayland oturumunda hiçbir şey yapmıyor. Ubuntu
ve güncel masaüstlerinin çoğu Wayland ile açılıyor. Hyperion çapraz platform
ama kurulumu ağır.

MasLight bunun yerine geçmeyi ve özellikle üç konuda daha iyi olmayı hedefliyor:

**Wayland.** Portal el sıkışması yazıldı; izin diyaloğunun her açılışta
çıkmasını engelleyen restore token dahil.

**HDR.** Windows HDR açıkken Desktop Duplication yarım kayan noktalı scRGB
veriyor. MasLight bunu çözüp parlak noktaları tone map ediyor; her aydınlık
sahneyi beyaza kırpmıyor.

**Kalibrasyon.** Saf kırmızı gönderip şeridin gerçekte hangi rengi
gösterdiğini soran bir sihirbaz. Kimse çipinin GRB mi RGB mi olduğunu tahmin
etmek zorunda kalmıyor. Bir de LED'leri kamerayla bulan sihirbaz: sekiz deseni
fotoğraflıyorsun, konumlar da kablolama sırası da fotoğraflardan çıkıyor;
kimsenin şeridini tarif etmesi gerekmiyor.

## Ne yapıyor

* **Ekran yakalamalı ambilight.** Renk hattı ilk pikselden son bayta kadar
  lineer ışıkta kalıyor.
* **Sese tepkili mod.** Hoparlörden çalan sesin loopback yakalaması, logaritmik
  bantlı spektrum, beat tespiti, dört efekt ve bir karışım kontrolü: müzik
  şeridin bir kısmını devralırken geri kalanı ekranı göstermeye devam eder.
* **Serbest yerleşim editörü.** Kenar başına LED sayısından yerleşim üret,
  sonra LED'leri tek tek sürükle. Çoklu monitör, ekran dışı LED'ler, zincirdeki
  boşluklar, ters takılmış şerit ve RGBW modelin parçası.
* **Yaygın protokollerin hepsi.** Tel üzerindeki baytlarını doğrulayan birim
  testleriyle: WLED UDP, DDP, sACN, Art-Net, Adalight, TPM2, OpenRGB ve ev
  otomasyonu için MQTT.
* **Otomatik cihaz keşfi.** mDNS ile bulur, sonra kontrolcüye kaç LED
  sürdüğünü sorar.
* **Profilleri kendi değiştiren kurallar**: çalışan bir program, tam ekran bir
  şey, saat aralığı ya da pilde olmak.
* **Yerel REST ve WebSocket API**: loopback üzerinde, token arkasında, sen
  açana kadar kapalı.
* **Script efektleri**: dosyalarına, ağına erişemeyen ve ışıkları
  kilitleyemeyen bir sandbox içinde.
* **Sisteme yük olmuyor.** 1080p ekran ve 60 LED için indirgeme ve renk hattı
  kare başına 0.035 ms, yani 60 fps'te bir çekirdeğin %0.2'si. Kendin ölçmek
  için: `cargo run --release -p maslight-core --example bench`.
  Sabit ekranda yakalama hızı kendiliğinden düşüyor;
  tam hızda bile geri okunan şey masaüstünün tamamı değil, küçük bir görüntü.
* **Hiçbir şey makineden çıkmıyor.** Hesap yok, analitik yok, çökme raporu yok.
  Güncelleme kontrolü varsayılan olarak kapalı.

## Kurulum

[Releases](https://github.com/maslight/maslight/releases) sayfasından paketini
indir; ayrıntılar [kurulum sayfasında](docs/install.md).

Sonrası: kontrolcünü tara, her ekran kenarı için LED sayısını gir, üç adımlık
kalibrasyonu çalıştır. Sönük şeritten çalışan şeride on dakika.

## Derleme

```sh
git clone https://github.com/maslight/maslight
cd maslight
npm install --prefix app
npm run build --prefix app
cargo run -p maslight-app
```

Sadece arayüzle uğraşmak için Rust bile gerekmiyor: `npm run dev --prefix app`
arayüzü demo bir arka uçla açar, donanım olmadan her ekran gezilebilir.

Visual Studio gerektirmeyen Windows derlemesi dahil tüm zincir ayrıntıları
[kaynaktan derleme](docs/building.md) sayfasında.

## Nasıl kurgulandı

```text
yakalama arka ucu -> bölge indirgeyici -> renk hattı -> çıkış
   (platform)          (LED başına)      (kare başına)  (bayt)
```

| Crate | İçerik |
|---|---|
| `maslight-core` | Renk hattı, yerleşim modeli, bölge indirgeyici, profiller. Platform kodu yok. |
| `maslight-capture` | `CaptureBackend` trait'i + DXGI, X11, PipeWire ve sentetik kaynak. |
| `maslight-output` | `Sink` trait'i + WLED, DDP, sACN, Art-Net, seri, keşif. |
| `maslight-audio` | Loopback yakalama, spektrum analizi, beat tespiti, efektler. |
| `maslight-rules` | Otomatik profil değişimi ve gereken platform sondaları. |
| `maslight-api` | Yerel REST ve WebSocket sunucusu. |
| `maslight-effects` | Script efektleri için sandbox'lı Rhai çalışma zamanı. |
| `maslight-calibrate` | Kamerayla konum keşfi ve gereken homografi. |
| `maslight-engine` | Döngü, profiller, telemetri, gecikme telafisi. |
| `app/` | Tauri kabuğu ve React arayüzü. |

Yükün çoğunu iki tasarım kararı taşıyor; ikisi de
[nasıl çalışıyor](docs/architecture.md) sayfasında anlatılıyor:

**Ortalama lineer ışıkta alınıyor.** Yarısı siyah yarısı beyaz bir ekran,
gerçekten yarı parlak bir orta griye ortalanır. Bunun yerine sRGB kodlarını
ortalarsan iki katından fazla parlak bir sonuç çıkar; bazı ambilight
yazılımlarının solgun görünmesinin sebebi bu.

**Küçültmeyi GPU yapıyor.** Windows'ta masaüstü tam mip zincirli bir dokuya
kopyalanır, piramidi GPU üretir, MasLight küçük bir mip'i geri okur:
1920x1080 masaüstü 240x135 olarak geliyor. Veri yolunda 64 kat daha az veri ve
mip'i üreten kutu filtresi zaten LED'lerin istediği ortalamanın ta kendisi.

## Henüz bitmeyenler

Bunu açıkça yazmak uzun bir özellik listesinden daha önemli:

* **Wayland yakalama** portal el sıkışmasından sonra duruyor. PipeWire okuyucu
  yazıldı ama Linux'ta hiç derlenmedi; bu yüzden bir feature bayrağının
  arkasında.
* **Linux derlemesinin fiziksel bir şeridi sürdüğünü kimse görmedi.** Derleniyor
  ve CI'da Xvfb altında gerçek bir X sunucusunu yakalıyor, ama bu aynı şey
  değil.
* **macOS** için yakalama arka ucu yok. Uygulama derleniyor ama yakalayacak bir
  şey yok. Bu 0.3 hedefi.
* **Philips Hue Entertainment** yapılmadı. DTLS el sıkışması gerekiyor; bu hem
  bir bağımlılık hem de kötü değil düzgün yapılmayı hak eden bir protokol.
* **Seri, OpenRGB ve MQTT** yazıldı ve sahte sunuculara karşı test edildi, ama
  hiçbiri henüz gerçek donanımla ya da gerçek bir broker'la buluşmadı.

## Dokümantasyon

* [Kurulum](docs/install.md)
* [Linux: X11 ve Wayland](docs/linux.md)
* [Ses modu](docs/audio.md)
* [Yerel API](docs/api.md)
* [Script efektleri](docs/scripting.md)
* [Çıkış protokolleri](docs/protocols.md)
* [Yerleşim modeli](docs/layout.md)
* [Kamerayla keşif](docs/discovery.md)
* [Kaynaktan derleme](docs/building.md)
* [Nasıl çalışıyor](docs/architecture.md)

## Katkı

[CONTRIBUTING.md](CONTRIBUTING.md) dosyasına bak. Kısası: `cargo test
--workspace --exclude maslight-app` geçmeli, `cargo clippy -- -D warnings`
sessiz olmalı, yeni bir protokol baytlarını doğrulayan bir test getirmeli.

## Lisans

MIT. [LICENSE](LICENSE) dosyasına bak.
