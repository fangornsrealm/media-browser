media-browser = Mediabläddrare
empty-folder = Tom katalog
empty-folder-hidden = Tom katalog (har dolda objekt)
no-results = Inga resultat hittades
filesystem = Filsystem
home = Hem
networks = Nätverk
notification-in-progress = Filoperationer pågår.
trash = Papperskorgen
recents = Senaste
undo = Ångra
today = Idag

# Listvy
name = Namn
modified = Modifierad
trashed-on = Kastat
size = Storlek

# Förloppssidfot
details = Detaljer
dismiss = Avvisa meddelande
operations-running = {$running} operationer körs ({$percent}%)...
operations-running-finished = {$running} operationer körs ({$percent}%), {$finished} färdigt...
pause = Paus
resume = Återuppta

# Dialoger

## Komprimera dialogruta
create-archive = Skapa arkiv

## Töm papperskorgen dialogruta
empty-trash = Töm papperskorgen
empty-trash-warning = Är du säker på att du vill ta bort alla objekt i papperskorgen permanent?

## Monteringsfel dialogruta
mount-error = Kan inte komma åt enheten

## Ny Fil/Katalog dialogruta
create-new-file = Skapa ny fil
create-new-folder = Skapa ny katalog
file-name = Filnamn
folder-name = Katalognamn
file-already-exists = En fil med det namnet existerar redan.
folder-already-exists = En katalog med det namnet existerar redan.
name-hidden = Namn som börjar med "." kommer att vara dolda.
name-invalid = Namn kan inte vara "{$filename}".
name-no-slashes = Namn får inte innehålla snedstreck.
recursive-scan-directories = Skanna alla underkataloger efter media

## Öppna/Spara dialogruta
cancel = Avbryt
create = Skapa
open = Öppna
open-file = Öppna fil
open-folder = Öppna katalog
open-in-new-tab = Öppna i ny flik
open-in-new-window = Öppna i nytt fönster
open-item-location = Öppna objektets plats
open-multiple-files = Öppna flera filer
open-multiple-folders = Öppna flera kataloger
save = Spara
save-file = Spara fil

## Öppna med dialogruta
open-with-title = Hur vill du öppna "{$name}"?
browse-store = Bläddra {$store}

## Byt namn på dialogruta
rename-file = Byt namn på fil
rename-folder = Byt namn på katalog

## Ersätt dialogruta
replace = Ersätt
replace-title = {$filename} finns redan på den här platsen.
replace-warning = Vill du ersätta den med den du sparar? Om du ersätter den kommer dess innehåll att skrivas över.
replace-warning-operation = Vill du ersätta den? Om du ersätter den kommer dess innehåll att skrivas över
original-file = Original fil
replace-with = Ersätt med
apply-to-all = Verkställ till alla
keep-both = Behåll båda
skip = Hoppa över

## Ställ in som körbar och startdialog
set-executable-and-launch = Ställ in som körbar och starta
set-executable-and-launch-description = Vill du ställa in "{$name}" som körbar och starta den?
set-and-launch = Ställ in och starta

## Metadata dialogruta
owner = Ägare
group = Grupp
other = Andra
read = Läs
write = Skriv
execute = Exekverbar

# Kontextsidor

## Om
git-description = Git commit {$hash} på {$date}

## Lägg till en Nätverksenhet
add-network-drive = Lägg till en Nätverksenhet
connect = Anslut
connect-anonymously = Anslut anonymt
connecting = Ansluter...
domain = Domän
enter-server-address = Ange server address
network-drive-description =
    Serveradresser inkluderar ett protokollprefix och en adress.
 Exempel: ssh://192.168.0.1, ftp://[2001:db8::1]
### Se till att behålla kommatecken som skiljer kolumnerna åt
network-drive-schemes =
    Tillgängliga protokoll, Prefix
    AppleTalk,afp://
    Filöverföringsprotokoll,ftp:// eller ftps://
    Nätverksfilsystem,nfs://
    Servermeddelandeblock,smb://
   SSH-filöverföringsprotokoll,sftp:// eller ssh://
    WebDav,dav:// eller davs://
network-drive-error = Kan inte komma åt nätverksenheten
password = Lösenord
remember-password = Kom ihåg lösenord
try-again = Försök igen
username = Användarnamn

## Operationer
cancelled = Avbruten
edit-history = Redigera historik
history = Historik
no-history = Inga objekt i historiken.
pending = Väntar
progress = {$percent}%
progress-cancelled = {$percent}%, avbruten
progress-paused = {$percent}%, pausad
failed = Misslyckades
complete = Färdig
copy_noun = Koperia
creating = Skapar "{$name}" i "{$parent}"
created = Skapade "{$name}" i "{$parent}"
copying = Kopierar {$items} {$items ->
        [one] objekt
        *[other] flera objekt
    } från "{$from}" till "{$to}" ({$progress})...
copied = Kopierade {$items} {$items ->
        [one] objekt
        *[other] flera objekt
    } från "{$from}" till "{$to}"
emptying-trash = Tömmer {trash} ({$progress})...
emptied-trash = Tömde {trash}
setting-executable-and-launching = Ställer in "{$name}" som exekverbar och startar
set-executable-and-launched = Ställ in "{$name}" som exekverbar och startar
moving = Flyttar {$items} {$items ->
        [one] objekt
        *[other] flera objekt
    } från "{$from}" till "{$to}" ({$progress})...
moved = Flyttade {$items} {$items ->
        [one] objekt
        *[other] flera objekt
    } från "{$from}" till "{$to}"
renaming = Byter namn "{$from}" till "{$to}"
renamed = Bytt namn "{$from}" till "{$to}"
restoring = Återställer {$items} {$items ->
        [one] objekt
        *[other] flera objekt
    } från {trash} ({$progress})...
restored = Återställt {$items} {$items ->
        [one] objekt
        *[other] flera objekt
    } från {trash}
unknown-folder = okänd katalog

## Öppna med
menu-open-with = Öppna med...
default-app = {$name} (default)

## Visa detaljer
show-details = Visa detaljer
type = Typ: {$mime}
items = Objekt: {$items}
item-size = Storlek: {$size}
item-created = Skapad: {$created}
item-modified = Modifierad: {$modified}
item-accessed = Åtkomst: {$accessed}
calculating = Beräknar...
item-media-release-date = Releasedatum: {$text}
item-media-size = Upplösning: {$width} x {$height}
item-media-runtime = Längd: {$text}
item-audio-languange = Språk: {$text}
item-subtitle-language = Undertexter: {$text}
item-media-actor = Skådespelare: {$text}
item-media-director = Regissör: {$text}
item-media-tag = Tagg: {$text}
item-media-album = Album: {$text}
item-media-composer = Kompositör: {$text}
item-media-genre = Genre: {$text}
item-media-artist = Artist: {$text}
item-media-albumartist = Album Artist: {$text}
item-image-lense-model = Linsmodell: {$text}
item-image-focal-length = Brännvidd: {$text}
item-image-exposure-time = Exponeringstid: {$text}
item-image-fnumber = Linsens beskärningsfaktor: {$text}
item-image-gps-latitude = GPS-latitud: {$text}
item-image-gps-longitude = GPS-longitud: {$text}
item-image-gps-altitude = GPS-höjd: {$text}
item-media-chapter = Kapitel: {$id}, från {$start} till {$end}

## Sök
search-context = Sök i databasen
search-previous = Tidigare söktermer
search-select = Välj
search-delete = Radera
search-mediatypes = Sök medietyper
search-images = Bilder
search-videos = Videor
search-audios = Ljudfiler
search-textentry = Ett fält för exakt matchning, båda för intervall
search-ranges = Sökintervall
search-text-from = Från / Minimalt
search-text-to = Till / Maximalt
search-value-from = Från / Minimalt (Nummer)
search-value-to = Till / Maximalt (Nummer)
search-tooltip-date = 
    Sök efter text, nummer eller datum
    Datum måste följa detta schema:
        Date i format YYYY-MM-DDThh:mm:ss
        Exempel:       2003-01-14T20:15:00
        Tiden kan utelämnas genom att endast ange datum 2003-01-14
search-tooltip-value = Numeriskt värde
search-filepath = Filsökväg
search-title = Titel
item-media-tag = Tagg: {$text}
search-description = Beskrivning
search-actor = Skådespelare/skådespelerska
search-director = Regissör
search-artist = Artist namn
search-album = Album namn
search-composer = Kompositörens namn
search-genre = Genre
search-album_artist = Album artist
search-duration = Längd (i sekunder)
search-creation_date = Skapandedatum
search-modification_date = Ändringsdatum
search-release_date = Releasedatum
search-lense_model = Linsmodell
search-focal_length = Brännvidd
search-exposure_time = Exponeringstid
search-fnumber = F-nummer
search-gps_latitude = GPS-latitud
search-gps_longitude = GPS-longitud
search-gps_altitude = GPS-höjd
search-commit = Starta sökning

## Inställningar
settings = Inställningar
settings-tab = Flik
settings-show-hidden = Visa dolda filer
default-view = Standardvy
icon-size-list = Ikonstorlek (lista)
icon-size-grid = Ikonstorlek (rutnät)
sorting-name = Sort efter
direction = Riktning
ascending = Stigande
descending = Fallande
metadata = Metadata / Database
metadata-delete = Delete Metadata / Database
metadata-details = {$items} Files with size {$size} MB
    at {$location}

### Likhetssök
similar-search = Likhetssök
similar-image-scan = Hitta liknande bilder
similar-video-scan = Hitta liknande videor
similar-image-sensitivity = Bildkänslighet
similar-video-sensitivity = Videokänslighet
similar-sensitivity-desc = Högre värden hittar fler, mindre likt sammanfall
similar-video-interval = Sekunder mellan skärmbilder
similar-video-min-sequence = Minsta antal likadana i följd
similar-video-threads = Skärmbildstrådar
similar-video-cache = Kandidatcache storlek
similar-video-cache-desc = ~100 MB RAM per kandidat; håll under en tredjedel av systemminnet
similar-recursive = Inkludera underkataloger
similar-delete-image-store = Rensa bildlikhetsdatabasen
similar-delete-video-store = Rensa videolikhetsdatabasen
similar-scan-running = Söker efter liknande {$media}…
similar-image-scan-complete = Hittade {$count} liknande bildpar
similar-video-scan-complete = Hittade {$count} liknande videor
similar-scan-cancelled = Likhetssök avbruten
similar-open-results = Öppna resultat
similar-ffmpeg-missing = ffmpeg krävs för videosökning men hittades inte.
similar-media-images = bilder
similar-media-videos = videor

### Utseende
appearance = Utseende
theme = Tema
match-desktop = Matcha skrivbordet
dark = Mörkt
light = Ljust

# Kontext meny
add-to-sidebar = Lägg till i sidofält
add-new-tag = Lägg till tagg i  sidofält
create-new-tag = Skapa ny tagg
tag-name = Taggnamn
compress = Komprimera
extract-here = Packa upp
new-file = Ny fil...
new-folder = Ny katalog...
open-in-terminal = Öppna i terminal
move-to-trash = Move to trash
restore-from-trash = Flytta till papperskorg
remove-from-sidebar = Ta bort från sidofält
remove-tag-from-sidebar = Ta bort tagg från sidofält
sort-by-name = Sortera efter namn
sort-by-modified = Sortera efter modifierad
sort-by-size = Sortera efter storlek

# Meny

## Fil
file = Fil
new-tab = Ny flik
new-window = Nytt fönster
rename = Rename...
menu-show-details = Visa detaljer...
close-tab = Stäng flik
quit = Avsluta

## Redigera
edit = Redigera
cut = Klipp ut
copy = Kopiera
paste = Klistra in
select-all = Välj alla

## Visa
zoom-in = Zooma in
default-size = Standardstorlek
zoom-out = Zooma ut
view = Visa
grid-view = Rutnätsvy
list-view = Listvy
show-hidden-files = Visa dolda filer
list-directories-first = Lista kataloger först
menu-settings = Inställningar...
menu-about = Om Mediabläddrare

## Sortera
sort = Sortera
sort-a-z = A-Z
sort-z-a = Z-A
sort-newest-first = Nyaste först
sort-oldest-first = Äldst först
sort-smallest-to-largest = Minsta till största
sort-largest-to-smallest = Största till minsta

# Knappar
button-back = Tillbaka
button-previous-file = Föregående
button-next-file = Nästa
button-play = Spela
button-pause = Paus
button-mute = Tysta
button-loop-on = Loop aktiv
button-loop-off = Loop av
button-subtitle = Undertexter
button-audio = Ljud
button-zoom-plus = Zooma in
button-zoom-minus = Zooma ut
button-zoom-fit = Passa vy
button-seek = ...

descripttion-back = Tillbaka till filbläddraren
description-previous-element = Föregående element i listan
description-next-element = Nästa element i listan
description-play = Spela upp filen
description-pause = Pausa uppspelningen
description-mute = Tysta ljudet
description-loop-on = Upprepa den aktuella filen
description-loop-off = Upprepa inte den aktuella filen
description-subtitle = Välj undertextström
description-audio = Välj ljudström
description-zoom-plus = Zooma in
description-zoom-minus = Zooma ut
description-zoom-fit = Anpassa bildvyn till fönsterstorleken
description-seek = Välj från grannbilder
description-seek-forward = Hoppa 30 sekunder framåt
description-seek-backward = Hoppa 30 sekunder bakåt
description-chapters = Välj kapitel att spela
description-streams = Välj strömmar att spela
description-browser = Välj andra media att använda

# Spelare
audio = Ljudström
subtitles = Undertextström
chapters = Kapitelmarkeringar
