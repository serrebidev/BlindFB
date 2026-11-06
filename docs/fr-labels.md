# Libellés de Facebook en français

Relevés le 2026-11-06 sur un compte Facebook en français (`<html lang="fr">`),
navigateur du backend `fb` 0.1.5-blindfb.34. « (relevé) » = vu sur la page ;
« (supposé) » = non observé, à confirmer.

Facebook écrit l'apostrophe `’` (U+2019), sépare les milliers par U+00A0 dans les prix,
et écrit les compteurs abrégés « 1,9 K » (virgule, espace, K majuscule).
Il mélange parfois les langues : un bouton Partager est resté en anglais
(« Share X's post, 18 shares ») au milieu d'une page en français.

## Publication (fil et permalien)

| Anglais | Français | |
|---|---|---|
| Comment on X's post / Comment, N comments | `Commenter la publication de X`, `Commenter la publication de X, 60 commentaires` | relevé |
| Like (post) | `Réagir avec J’aime à la publication de X` | relevé |
| React | `Réagir à la publication de X` ; dans un commentaire : `Réagir` | relevé |
| Share | `Partager la publication de X` (ou anglais `Share X's post, 18 shares`) | relevé |
| Actions for this post | `Actions pour cette publication de X` | relevé |
| Hide post | `Masquer la publication de X` | relevé |
| Reaction summary | `J’aime : 5 personnes`, `J’adore : 1 personne`, `Haha : 25 personnes` | relevé |
| N reactions: see who reacted | `27 réactions : voir qui a réagi`, `1 réaction : voir qui a réagi` | relevé |
| Comment article | `[role=article][aria-label="Commentaire de X il y a 8 heures"]` | relevé |
| Comment box | `Commenter en tant que <Nom>` | relevé |
| Like / Reply / Edited (comment) | `J’aime`, `Répondre`, `Modifié` | relevé |
| View N replies | `Voir les 3 réponses` | relevé |
| Most relevant | `Plus pertinents` | relevé |
| See more | `En voir plus` (et toujours `Voir plus` ailleurs) | relevé |
| Post comment | `Publier le commentaire` | relevé |
| Close | `Fermer` | relevé |
| Sponsored menu | `Ouvrez le menu pour le contenu sponsorisé de X` | relevé |
| Verified badge | `Compte vérifié` (texte), long : `Les comptes avec un badge Vérifié ont été authentifiés. Il peut s’agir d’abonnés Meta Verified ou de personnes ou de marques notables.` | relevé |
| Follow / Following | `Suivre`, `Vous suivez X` | relevé |
| Remove Like / Unlike | `Retirer J’aime` ou `Supprimer J’aime` | supposé |

## Partage (relevé en mode manuel, rien n'a été partagé)

Le bouton Partager ouvre d'abord une fenêtre de choix, puis un éditeur :

| Étape | Français | |
|---|---|---|
| Fenêtre de choix | `[role=dialog][aria-label="Options de partage"]` : `Partager sur le Fil`, `Partager dans votre story (Ami(e)s)`, `Envoyer dans Messenger`, `Partager dans un groupe`, `Partager sur le profil d’un(e) ami(e)` | relevé |
| Éditeur (Share to Feed) | dialogue `Écrire une publication`, zone `Quoi de neuf, <Prénom> ?`, bouton texte `Partager` (pas « Partager maintenant ») | relevé |
| Audience | `Modifier la confidentialité. Partage avec Vos ami(e)s.` | relevé |
| Fermer l'éditeur | `Fermer la boîte de dialogue de l’éditeur`, puis `Enregistrer comme brouillon` / `Supprimer le brouillon` / `Fermer` | relevé |

Le bouton Partager d'une publication ouverte en fenêtre ne réagit pas à un clic
simulé en JavaScript (`click()` ni suite d'événements pointeur) : BlindFB le fait
cliquer par le navigateur (`fb page click`, référence donnée par `shareTarget`).
Vérifié sur un vrai compte : ouverture, audience lue (« Vos ami(e)s »), fermeture et
brouillon abandonné ; le bouton final « Partager » n'a pas été pressé.

## Reel et vidéo (lecteur plein écran, `/reel/…` et `/watch/`)

| Anglais | Français | |
|---|---|---|
| Comment, N comments | `Commenter, 2 commentaires`, `Commenter, 917 commentaires` | relevé |
| Like, N reactions | `J’aime, 28 réactions`, `J’aime, 1,9 K réactions`, `J’aime, 40,1 K réactions` | relevé |
| React | `Réagir` | relevé |
| Share, N shares | `Partager, 4 partages`, `Partager, 3,2 K partages` | relevé |
| More options | `Plus d’options` | relevé |
| Next Card / Previous Card | `Carte suivante`, `Carte précédente` | relevé |
| owner profile | `Voir profil du propriétaire`, `X, voir le profil du propriétaire` | relevé |
| Shared with Public | `Partagé avec : Public`, `Partagé avec : Groupe public` | relevé |
| Close player | `Fermer le lecteur et revenir au fil` | relevé |
| Unmute | `Réactiver` | relevé |
| Video player / Change Position | `Video player`, `Change Position` (restés en anglais) | relevé |
| Comments panel | `[role=complementary]` avec des `[role=article]` comme ci-dessus | relevé |
| Reaction picker | dialogue `Réactions` : `J’aime`, `J’adore`, `Solidaire`, `Haha`, `Wouah`, `Triste`, `En colère` | relevé |
| Leave a comment (Watch) | non vu : `/watch/` ouvre le même lecteur que les Reels | — |

## Temps et nombres

| Contexte | Formes |
|---|---|
| Publications, commentaires | `il y a 10 heures`, `il y a 19 minutes`, `8 h`, `4 h` |
| Liste Messenger | `2 min`, `2 h`, `21 h`, `1 j`, `1 sem.`, `7 sem.` (point final) |
| Compteurs | `1,9 K`, `40,1 K`, `3,2 K` |
| Prix Marketplace | `35 €`, `4 890 €`, `39,99 €` (U+00A0 avant € et entre milliers) |

## Messenger et Marketplace

| Anglais | Français | |
|---|---|---|
| More options for X | `Autres options pour X` | relevé |
| Settings, help and more | `Paramètres, aide et autres` | relevé |
| New message | `Nouveau message` | relevé |
| Use a one-time code instead | `Utiliser plutôt un code à usage unique` | relevé |
| Location | `Lieu : Toulouse, Dans un rayon de 500 km` | relevé |
| Free | `Gratuit` (à la place du prix) | relevé |
| Just listed (badge) | `Annonce récente`, avant le prix | relevé |
