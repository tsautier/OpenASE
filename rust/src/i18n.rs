//! English and French interface texts.
//!
//! Source code keeps English literals (`tr("Refresh")`); `tr` returns the
//! French text when French is active and the input unchanged otherwise, so a
//! string without a French entry simply shows in English. Strings with values
//! use `{}` placeholders filled by [`fill`], one entry per whole sentence.
//! `tests/i18n.rs` fails when a `tr("...")` literal has no French entry.

use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

/// Interface language chosen in Options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    /// French when the operating system is set to French, English otherwise.
    #[default]
    System,
    English,
    French,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::System, Language::English, Language::French];

    /// Name shown in the language list (the language names are not translated).
    pub fn label(self) -> &'static str {
        match self {
            Language::System => noop("System"),
            Language::English => "English",
            Language::French => "Français",
        }
    }

    pub fn is_french(self) -> bool {
        match self {
            Language::System => system_is_french(),
            Language::English => false,
            Language::French => true,
        }
    }
}

static FRENCH: AtomicBool = AtomicBool::new(false);

/// Make [`tr`] follow `language` from now on.
pub fn set_language(language: Language) {
    FRENCH.store(language.is_french(), Ordering::Relaxed);
}

/// Is the interface currently in French?
pub fn is_french() -> bool {
    FRENCH.load(Ordering::Relaxed)
}

/// Translate an English interface text (identity when French is not active).
pub fn tr(text: &'static str) -> &'static str {
    if is_french() {
        french(text).unwrap_or(text)
    } else {
        text
    }
}

/// Marks a text for translation where `tr` cannot be called yet (constants and
/// tables): the call site then runs `tr` on the value.
pub const fn noop(text: &'static str) -> &'static str {
    text
}

/// Replace the `{}` placeholders of `template`, in order, with `args`.
pub fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut rest = template;
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        match args.next() {
            Some(arg) => out.push_str(&arg.to_string()),
            None => out.push_str("{}"),
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// Display name of a favorite or friend group ("Favorites1" -> "Favoris1").
pub fn group_name(group: &str) -> String {
    for (english, fallback) in [("Favorites", "Favorites"), ("Friends", "Friends")] {
        if let Some(number) = group.strip_prefix(english) {
            let shown = if is_french() {
                french(fallback).unwrap_or(fallback)
            } else {
                fallback
            };
            return format!("{shown}{number}");
        }
    }
    group.to_string()
}

/// Is the operating system interface in French?
pub fn system_is_french() -> bool {
    system_language().is_some_and(|code| code.to_ascii_lowercase().starts_with("fr"))
}

#[cfg(windows)]
fn system_language() -> Option<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // SAFETY: plain query without arguments.
    let id = unsafe { GetUserDefaultUILanguage() };
    // Primary language 0x0c is French, whatever the region.
    Some(if id & 0x3ff == 0x0c { "fr" } else { "en" }.to_string())
}

#[cfg(not(windows))]
fn system_language() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
}

/// French text of an English source string, `None` when there is none.
///
/// Placeholders (`{}`) must appear in the same number as in the English text.
/// Texts that read the same in French are listed too, so that the test can
/// tell them from forgotten ones.
pub fn french(text: &str) -> Option<&'static str> {
    Some(match text {
        // Names and words that do not change.
        "OpenASE" => "OpenASE",
        "Ping" => "Ping",
        "Flux" => "Flux",
        "DSL" => "DSL",
        "Score" => "Score",
        "Version {}" => "Version {}",
        "   OK   " => "   OK   ",

        // Menus.
        "File" => "Fichier",
        "View" => "Affichage",
        "Filter" => "Filtre",
        "Server" => "Serveur",
        "Player" => "Joueur",
        "Friends" => "Amis",
        "Tools" => "Outils",
        "Help" => "Aide",
        "Toolbar" => "Barre d’outils",
        "Export user profile..." => "Exporter le profil utilisateur…",
        "Export user profile" => "Exporter le profil utilisateur",
        "Import user profile..." => "Importer le profil utilisateur…",
        "Import user profile" => "Importer le profil utilisateur",
        "OpenASE profile" => "Profil OpenASE",
        "Save server list..." => "Enregistrer la liste des serveurs…",
        "Save server list" => "Enregistrer la liste des serveurs",
        "Import server list..." => "Importer une liste de serveurs…",
        "Import server list" => "Importer une liste de serveurs",
        "Check for updates" => "Rechercher des mises à jour",
        "Automatic update check" => "Recherche automatique des mises à jour",
        "Setup Wizard..." => "Assistant de configuration…",
        "Setup Wizard" => "Assistant de configuration",
        "Exit" => "Quitter",
        "Refresh\tF5" => "Actualiser\tF5",
        "Show selected\tCtrl+E" => "Afficher la sélection\tCtrl+E",
        "Clear modifiers\tCtrl+D" => "Effacer les modificateurs\tCtrl+D",
        "Collapse all\tCtrl+Q" => "Tout réduire\tCtrl+Q",
        "Not empty" => "Non vides",
        "Not full" => "Non pleins",
        "No password" => "Sans mot de passe",
        "Hide unresponsive servers" => "Masquer les serveurs sans réponse",
        "New filter..." => "Nouveau filtre…",
        "Edit filter..." => "Modifier le filtre…",
        "Make color filter..." => "Créer un filtre de couleur…",
        "Delete filter" => "Supprimer le filtre",
        "Configure games..." => "Configurer les jeux…",
        "Add friend..." => "Ajouter un ami…",
        "Manage friends..." => "Gérer les amis…",
        "Friend name search..." => "Recherche d’un nom d’ami…",
        "Show servers with friends" => "Afficher les serveurs avec des amis",
        "OpenASE Help\tF1" => "Aide d’OpenASE\tF1",
        "License" => "Licence",
        "Online manual" => "Manuel en ligne",
        "Message board" => "Forum de discussion",
        "Website" => "Site web",
        "About OpenASE..." => "À propos d’OpenASE…",
        "About OpenASE" => "À propos d’OpenASE",
        "Refresh list\tF5" => "Actualiser la liste\tF5",
        "Join\tEnter" => "Rejoindre\tEntrée",
        "Copy address" => "Copier l’adresse",
        "Add server..." => "Ajouter un serveur…",
        "Find LAN servers" => "Chercher des serveurs LAN",
        "Find player..." => "Rechercher un joueur…",
        "Options..." => "Options…",

        // Toolbar and its menu.
        "Refresh server list" => "Actualiser la liste des serveurs",
        "Refresh server list (F5)" => "Actualiser la liste des serveurs (F5)",
        "Stop" => "Arrêter",
        "Refresh selected server" => "Actualiser le serveur sélectionné",
        "Ping visible servers" => "Pinger les serveurs visibles",
        "Query the servers shown in the list (at most {})" => {
            "Interroger les serveurs affichés dans la liste ({} au maximum)"
        }
        "Join server" => "Rejoindre le serveur",
        "Join server (Enter)" => "Rejoindre le serveur (Entrée)",
        "Add to / remove from favorites" => "Ajouter aux favoris / retirer des favoris",
        "Add server" => "Ajouter un serveur",
        "Find player" => "Rechercher un joueur",
        "Options" => "Options",
        "Customize toolbar..." => "Personnaliser la barre d’outils…",
        "Reset toolbar" => "Réinitialiser la barre d’outils",

        // Filter tree.
        "Favorites" => "Favoris",
        "{} ({} on)" => "{} ({} en ligne)",
        "Special" => "Spécial",
        "recently added" => "ajoutés récemment",
        "recently imported" => "importés récemment",
        "LAN servers" => "Serveurs LAN",
        "recently joined" => "rejoints récemment",
        "My filters" => "Mes filtres",
        "{} (color)" => "{} (couleur)",
        "(refresh to fill)" => "(actualisez pour remplir)",
        "(none)" => "(aucun)",
        "Modifiers" => "Modificateurs",
        "Players" => "Joueurs",
        "Hide unresponsive" => "Masquer sans réponse",
        "less than {}" => "moins de {}",
        "Country" => "Pays",
        "Country *" => "Pays *",

        // Server list, panes and context menu.
        "Location" => "Lieu",
        "Name" => "Nom",
        "PL" => "PP",
        "Game" => "Jeu",
        "Map" => "Carte",
        "Address" => "Adresse",
        "<- select a game and click Refresh, or add a server" => {
            "<- choisissez un jeu et cliquez sur Actualiser, ou ajoutez un serveur"
        }
        "Join" => "Rejoindre",
        "Refresh" => "Actualiser",
        "Remove from favorites" => "Retirer des favoris",
        "Add to favorites" => "Ajouter aux favoris",
        "Add to favorites group" => "Ajouter au groupe de favoris",
        "Friend - on" => "Amis - en ligne",
        "Friend - off" => "Amis - hors ligne",
        "Player Name" => "Nom du joueur",
        "Time" => "Temps",
        "Key" => "Clé",
        "Value" => "Valeur",
        "(no response)" => "(pas de réponse)",
        "unscanned" => "non analysé",
        "Unknown" => "Inconnu",

        // Status bar and notices.
        "Ready" => "Prêt",
        "Loading..." => "Chargement…",
        "Disabled" => "Désactivée",
        "{} is playing on {}" => "{} joue sur {}",
        "servers" => "serveurs",
        "known servers" => "serveurs connus",
        "visible servers" => "serveurs visibles",
        "imported servers" => "serveurs importés",
        "Looking for servers on the local network..." => {
            "Recherche de serveurs sur le réseau local…"
        }
        "Scanning {}" => "Analyse de {}",
        "Querying {} master servers..." => "Interrogation des serveurs maîtres de {}…",
        "No {} to refresh" => "Pas de {} à actualiser",
        "Refreshing {}" => "Actualisation des {}",
        "Invalid address {}" => "Adresse {} non valide",
        "No server list could be downloaded:\n\n{}" => {
            "Aucune liste de serveurs n’a pu être téléchargée :\n\n{}"
        }
        "Some masters failed: {}" => "Certains serveurs maîtres ont échoué : {}",
        "Scan stopped" => "Analyse arrêtée",
        "Scan complete" => "Analyse terminée",
        "Refreshed {}" => "{} actualisé",
        "{} did not answer" => "{} n’a pas répondu",
        "{} IPv4 ranges loaded" => "{} plages IPv4 chargées",
        "Not available: {}" => "Indisponible : {}",
        "GeoIP database not available: {}" => "Base GeoIP indisponible : {}",
        "No server found on the local network" => "Aucun serveur trouvé sur le réseau local",
        "LAN search failed: {}" => "Échec de la recherche LAN : {}",
        "OpenASE {} is up to date." => "OpenASE {} est à jour.",
        "Could not check for updates: {}" => "Impossible de rechercher les mises à jour : {}",
        "Launched {}" => "{} lancé",
        "Opened {}" => "{} ouvert",
        "Added to {}" => "Ajouté à {}",
        "Copied {}" => "{} copié",
        "Unknown game" => "Jeu inconnu",
        "Could not save settings: {}" => "Impossible d’enregistrer les paramètres : {}",
        "{} servers, {}/{} players total, {} visible" => {
            "{} serveurs, {}/{} joueurs au total, {} visibles"
        }
        "{} of {} servers, {}/{} players total, {} visible" => {
            "{} serveurs sur {}, {}/{} joueurs au total, {} visibles"
        }
        "Load more servers ({} of {})" => "Charger plus de serveurs ({} sur {})",
        "Loading more servers..." => "Chargement d’autres serveurs…",
        "{} more servers loaded" => "{} serveurs de plus chargés",
        "Could not load more servers: {}" => "Impossible de charger plus de serveurs : {}",
        "Modifiers cleared" => "Modificateurs effacés",
        "Profile exported to {}" => "Profil exporté vers {}",
        "Export failed: {}" => "Échec de l’exportation : {}",
        "Profile imported from {}" => "Profil importé depuis {}",
        "Import failed: {}" => "Échec de l’importation : {}",
        "Deleted filter {}" => "Filtre {} supprimé",
        "The server list is empty." => "La liste de serveurs est vide.",
        "Saved {} servers to {}" => "{} serveurs enregistrés dans {}",
        "Could not save: {}" => "Enregistrement impossible : {}",
        "{} added to your friends" => "{} ajouté à vos amis",
        "No ip:port entries found in this file." => {
            "Aucune entrée ip:port trouvée dans ce fichier."
        }
        "Imported {} servers into {}" => "{} serveurs importés dans {}",
        "Cannot read {}: {}" => "Impossible de lire {} : {}",
        "Set the program location of {} in Options > Games first." => {
            "Indiquez d’abord l’emplacement du programme de {} dans Options > Jeux."
        }

        // Dialogs: About, Add server, password, friends, find player.
        "An open-source game server browser in the spirit of The All-Seeing Eye." => {
            "Un navigateur de serveurs de jeu open source, dans l’esprit de The All-Seeing Eye."
        }
        "New code, not affiliated with Yahoo or the original authors." => {
            "Code entièrement nouveau, sans lien avec Yahoo ni les auteurs d’origine."
        }
        "Free software under the GNU GPL v3 or later." => {
            "Logiciel libre sous licence GNU GPL v3 ou ultérieure."
        }
        "Server lists from 333networks, OpenSpy, dpmaster and the other community master servers." => {
            "Listes de serveurs fournies par 333networks, OpenSpy, dpmaster et les autres serveurs maîtres communautaires."
        }
        "Add Server" => "Ajouter un serveur",
        "Cancel" => "Annuler",
        "Close" => "Fermer",
        "   Add   " => "  Ajouter  ",
        "   Join   " => "  Rejoindre  ",
        "Password required" => "Mot de passe requis",
        "This server needs a password." => "Ce serveur demande un mot de passe.",
        "OpenASE looks for these names on every server it scans." => {
            "OpenASE recherche ces noms sur chaque serveur analysé."
        }
        "No friends yet." => "Aucun ami pour l’instant.",
        "online" => "en ligne",
        "offline" => "hors ligne",
        "Remove" => "Retirer",
        "Add" => "Ajouter",
        "Player name" => "Nom du joueur",
        "Find Player" => "Rechercher un joueur",
        "Player name contains" => "Le nom du joueur contient",
        "{} players found in scanned servers" => "{} joueurs trouvés sur les serveurs analysés",
        "Searching for {}" => "Recherche de {}",
        "No installation found." => "Aucune installation trouvée.",
        "Search complete." => "Recherche terminée.",
        "Select a sound" => "Choisir un son",
        "WAV sound" => "Son WAV",

        // Options.
        "General" => "Général",
        "Network" => "Réseau",
        "Events" => "Événements",
        "Games" => "Jeux",
        "Colors" => "Couleurs",
        "Language" => "Langue",
        "System" => "Système",
        "Counter-Strike 1.6 server lists come from the Steam Web API, which needs your own free key." => {
            "Les listes de serveurs de Counter-Strike 1.6 viennent de l’API Web Steam, qui demande votre propre clé gratuite."
        }
        "Steam Web API key" => "Clé de l’API Web Steam",
        "Get a key at steamcommunity.com/dev/apikey" => {
            "Obtenir une clé sur steamcommunity.com/dev/apikey"
        }
        "Show server countries (downloads a free IP-to-country database once a month)" => {
            "Afficher le pays des serveurs (télécharge une base de données IP-pays gratuite une fois par mois)"
        }
        "Database: {}." => "Base de données : {}.",
        "Takes effect at the next start." => "Prend effet au prochain démarrage.",
        "Settings file" => "Fichier de paramètres",
        "Advanced connection settings" => "Paramètres de connexion avancés",
        "Timeout when querying servers (ms)" => "Délai d’attente des requêtes aux serveurs (ms)",
        "Number of times to ping a server" => "Nombre de pings par serveur",
        "Number of times to retry a scan" => "Nombre de nouvelles tentatives d’une analyse",
        "Servers queried at the same time" => "Serveurs interrogés simultanément",
        "Maximum servers per refresh" => "Nombre maximal de serveurs par actualisation",
        "Restore defaults" => "Rétablir les valeurs par défaut",
        "Ping is the average round-trip time, Flux half the spread between the fastest and slowest answer, PL the share of unanswered queries." => {
            "Le ping est le temps d’aller-retour moyen, le flux la moitié de l’écart entre la réponse la plus rapide et la plus lente, PP la part de requêtes sans réponse."
        }
        "Friend joins a server" => "Un ami rejoint un serveur",
        "Show a notice" => "Afficher une notification",
        "Play the system notification sound" => "Jouer le son de notification du système",
        "Monitored server" => "Serveur surveillé",
        "Refresh the selected server every" => "Actualiser le serveur sélectionné toutes les",
        "seconds" => "secondes",
        "Installed games" => "Jeux installés",
        "Not installed" => "Non installés",
        "Select a game on the left." => "Sélectionnez un jeu à gauche.",
        "Visible in filter list" => "Visible dans la liste des filtres",
        "Config file" => "Fichier de configuration",
        "Browse" => "Parcourir",
        "Browse..." => "Parcourir…",
        "Select config file" => "Choisir le fichier de configuration",
        "Program location" => "Emplacement du programme",
        "Select game executable" => "Choisir l’exécutable du jeu",
        "Executable" => "Exécutable",
        "All files" => "Tous les fichiers",
        "Auto-Search" => "Recherche auto",
        "allow invitations for this game" => "autoriser les invitations pour ce jeu",
        "draw player names with game colors" => {
            "afficher les noms des joueurs avec les couleurs du jeu"
        }
        "Default sound" => "Son par défaut",
        "No sound" => "Aucun son",
        "Sound file..." => "Fichier son…",
        "Path of a .wav file" => "Chemin d’un fichier .wav",
        "Play" => "Écouter",
        "Choose the buttons of the toolbar and their order. You can also right-click the toolbar." => {
            "Choisissez les boutons de la barre d’outils et leur ordre. Vous pouvez aussi faire un clic droit sur la barre d’outils."
        }
        "Up" => "Monter",
        "Down" => "Descendre",
        "Reset to default" => "Rétablir par défaut",
        "Changes are shown right away; Cancel puts the old colors back." => {
            "Les changements s’affichent tout de suite ; Annuler rétablit les anciennes couleurs."
        }
        "List background" => "Fond de la liste",
        "Alternating row" => "Ligne alternée",
        "Selection" => "Sélection",
        "Text" => "Texte",
        "Draw Quake color codes in server and player names" => {
            "Afficher les codes couleur Quake dans les noms de serveurs et de joueurs"
        }
        "Reset to defaults" => "Rétablir les valeurs par défaut",

        // Setup wizard.
        "Welcome to OpenASE!" => "Bienvenue dans OpenASE !",
        "This is your first time running OpenASE, so let's set a few things up." => {
            "C’est la première fois que vous lancez OpenASE : configurons quelques réglages."
        }
        "First, tell OpenASE about your internet connection." => {
            "Commencez par indiquer votre connexion internet à OpenASE."
        }
        "Refreshing a server list sends thousands of small queries; matching them to your connection keeps pings accurate." => {
            "Actualiser une liste de serveurs envoie des milliers de petites requêtes ; les adapter à votre connexion garde des pings précis."
        }
        "You can run this wizard again from File > Setup Wizard." => {
            "Vous pouvez relancer cet assistant depuis Fichier > Assistant de configuration."
        }
        "Your connection" => "Votre connexion",
        "Slow (mobile, satellite)" => "Lente (mobile, satellite)",
        "Cable or fiber (recommended)" => "Câble ou fibre (recommandé)",
        "Very fast, low latency" => "Très rapide, faible latence",
        "{} servers at once, {} ms timeout" => "{} serveurs à la fois, délai de {} ms",
        "If pings look too high, come back here and pick a slower setting. Fine tuning lives in Options > Network." => {
            "Si les pings semblent trop élevés, revenez ici et choisissez un réglage plus lent. Le réglage fin se trouve dans Options > Réseau."
        }
        "You and your games" => "Vous et vos jeux",
        "Games shown in the filter tree:" => "Jeux affichés dans l’arbre des filtres :",
        "{}  (installed)" => "{}  (installé)",
        "Setup done." => "Configuration terminée.",
        "Tips:" => "Astuces :",
        "•  F5 refreshes the selected game, double-click or Enter joins a server." => {
            "•  F5 actualise le jeu sélectionné ; double-clic ou Entrée pour rejoindre un serveur."
        }
        "•  Tick filters in the tree: game types, mods, maps, Kill Cam, countries..." => {
            "•  Cochez des filtres dans l’arbre : types de jeu, mods, cartes, Kill Cam, pays…"
        }
        "•  Friends > Add friend tracks players on every server you scan." => {
            "•  Amis > Ajouter un ami suit des joueurs sur chaque serveur analysé."
        }
        "•  Counter-Strike 1.6 lists need a free Steam Web API key (Options > General)." => {
            "•  Les listes de Counter-Strike 1.6 demandent une clé d’API Web Steam gratuite (Options > Général)."
        }
        "Have fun, and see you on the servers." => {
            "Amusez-vous bien, et à bientôt sur les serveurs."
        }
        "Next >" => "Suivant >",
        "Finish" => "Terminer",
        "< Back" => "< Précédent",

        // Add friend, import, filter editor, license, update.
        "Add Friend" => "Ajouter un ami",
        "Enter the player's name as it appears on servers. Color codes are ignored and partial names match." => {
            "Saisissez le nom du joueur tel qu’il apparaît sur les serveurs. Les codes couleur sont ignorés et les noms partiels conviennent."
        }
        "Go to Friend Name Search" => "Aller à la recherche d’un nom d’ami",
        "Group" => "Groupe",
        "Server list" => "Liste de serveurs",
        "Add to" => "Ajouter à",
        "Any text file with ip:port entries works: OpenASE lists, GameSpy exports, configs..." => {
            "Tout fichier texte contenant des entrées ip:port convient : listes OpenASE, exports GameSpy, configurations…"
        }
        "  Import  " => "  Importer  ",
        "Edit filter" => "Modifier le filtre",
        "New filter" => "Nouveau filtre",
        "My filter" => "Mon filtre",
        "Any game" => "Tous les jeux",
        "Show servers where all of these are true:" => {
            "Afficher les serveurs qui remplissent toutes ces conditions :"
        }
        "Any server rule, e.g. g_gametype" => "Toute règle de serveur, p. ex. g_gametype",
        "+ Add rule" => "+ Ajouter une règle",
        "Color filter: highlight matching servers everywhere" => {
            "Filtre de couleur : met en évidence partout les serveurs correspondants"
        }
        "is" => "est",
        "is not" => "n’est pas",
        "less than" => "inférieur à",
        "greater than" => "supérieur à",
        "contains" => "contient",
        "does not contain" => "ne contient pas",
        "OpenASE is free software under the GNU General Public License, version 3 or later." => {
            "OpenASE est un logiciel libre sous licence GNU General Public License, version 3 ou ultérieure."
        }
        "Update available" => "Mise à jour disponible",
        "OpenASE {} is available. You are running {}." => {
            "OpenASE {} est disponible. Vous utilisez la version {}."
        }
        "Download it from GitHub Releases" => "Télécharger depuis les versions GitHub",
        "Later" => "Plus tard",
        _ => return None,
    })
}
