// Facebook's own words for every button, badge and time BlindFB looks for, in each
// language seen so far (English, French, Vietnamese). Facebook mixes languages on one
// page — a French page can keep an English Share button — so every pattern accepts
// all of them. French values come from docs/fr-labels.md. Facebook writes the
// apostrophe as ’ (U+2019); patterns accept ' as well.
(() => {
  const A = "['’]";
  const exact = (body, flags = 'i') => new RegExp('^(?:' + body + ')$', flags);
  // "28", "1.2K", "1,9 K", "4 890" (with an ordinary, no-break or narrow no-break space).
  const COUNT = '[\\d.,]+(?:[\\s\\u00a0\\u202f]\\d{3})*(?:[\\s\\u00a0\\u202f]?[KMBkmb])?';
  // Reaction names on the picker and on a reacted Like button, per BlindFB keyword.
  const REACTIONS = {
    like: ['Like', `J${A}aime`, 'Thích'],
    love: ['Love', `J${A}adore`, 'Yêu thích'],
    care: ['Care', 'Solidaire', 'Thương thương'],
    haha: ['Haha'],
    wow: ['Wow', 'Wouah'],
    sad: ['Sad', 'Triste', 'Buồn'],
    angry: ['Angry', 'En colère', 'Grrr', 'Phẫn nộ'],
  };
  const REMOVE = '(?:Remove|Retirer|Supprimer|Annuler)\\s+';

  const L = {
    COUNT,
    // A reel or video card's comment button: "Comment", "Comment, 1.2K comments",
    // "Commenter", "Commenter, 1,9 K commentaires".
    comment: exact('Comment(?:, .+ comments?)?|Commenter(?:, .+ commentaires?)?'),
    // Opens the comments of a Watch video ("Leave a comment" in English).
    leaveComment: exact('Leave a comment|Laissez un commentaire|Laisser un commentaire', ''),
    seeMore: exact('See more|Xem thêm|Voir plus|En voir plus|Afficher la suite'),
    // Words in a comment that are its actions, not its text.
    commentAction: exact(`Like|Reply|Edited|See translation|Thích|Trả lời|J${A}aime|Répondre|Modifié|Voir la traduction`),
    // Badges on a comment's author line.
    commentBadge: exact('(?:by )?author|Top fan|Follow|Auteur|Fan assidu|Super fan|Suivre|·'),
    // A comment's own Like button, liked or not.
    commentLike: exact(`Like|Remove Like|Unlike|J${A}aime|${REMOVE}J${A}aime|Je n${A}aime plus`),
    reply: exact('Reply|Trả lời|Répondre'),
    replyStart: /^(?:Reply|Trả lời|Répondre)\b/i,
    // Buttons that load a comment's hidden replies: "View replies", "View 3 replies",
    // "View more replies". Never "Hide replies" — that would collapse them again.
    viewReplies: exact(`View(?: more| previous)?(?: ${COUNT})? replies|View 1 reply|Afficher(?: les| plus de)?(?: ${COUNT})? réponses|Voir(?: les| plus de)?(?: ${COUNT})? réponses|Xem(?: thêm)?(?: ${COUNT})?(?: câu)? trả lời`),
    // "Share", "Share X's post, 18 shares", "Partager la publication de X", "Partager, 4 partages".
    share: /^(?:Share|Partager)\b/i,
    shareNow: exact('Share now|Partager maintenant|Partager'),
    // French Facebook first asks where to share; "to the feed" opens the editor with Partager.
    shareOptions: exact('Share options|Options de partage'),
    shareToFeed: exact('Share to Feed|Partager sur le Fil'),
    // Closing an unsent share asks what to do with its draft.
    discardDraft: exact('Discard|Discard post|Supprimer le brouillon'),
    editPrivacy: /^(?:Edit privacy|Modifier la confidentialité)/i,
    sharingWith: /(?:Sharing with|Partag[ée] avec\s*:?)\s*(.+?)\.?$/i,
    close: exact(`Close|Fermer|Fermer la boîte de dialogue de l${A}éditeur`),
    react: exact('React|Réagir', ''),
    reactionsDialog: exact('Reactions|Réactions'),
    // A notification about several reels at once: "X posted 3 new reels" (French form supposed).
    manyReels: /\s(?:new reels|nouveaux reels|nouvelles vidéos reels)\b/i,
    nextReel: exact('Next Card|Next reel|Carte suivante|Reel suivant'),
    // Lines of a reel card that are not its caption: badges, Follow and counts.
    captionNoise: exact(`·|Follow|Following|Verified account|Suivre|Abonné(?:e)?|Vous suivez .+|Compte vérifié|${COUNT}`),
    // Post and comment times, when the backend does not supply its own pattern.
    time: /^(?:\d+\s*(?:s|m|h|d|w|y|min|hr|day|week)s?|Just now|il y a .+|\d+\s*(?:j|sem\.?|ans?)|À l’instant|À l'instant|Hier.*|Aujourd’hui.*|Aujourd'hui.*)$/i,
    // The account named by a reels page title: "X Reels | Facebook", French supposed "Reels de X".
    reelsTitleAuthor: title => String(title || '').replace(/ \| Facebook$/, '').replace(/ Reels.*$/, '').replace(/^Reels (?:de |d’|d')/i, ''),
    // The picker button for one reaction keyword.
    reactionName: kind => exact((REACTIONS[kind] || []).join('|')),
    // What a reel's Like button says about the current reaction: 'none', a keyword, or
    // null when the label is not a Like button. Counts after a comma are ignored
    // ("J’aime, 28 réactions"). French may name the reaction itself instead of
    // "Remove <reaction>", so a bare non-Like name counts as that reaction.
    reelReaction(text, pressed) {
      const bare = String(text || '').replace(/,\s.*$/, '').trim();
      const removal = new RegExp('^' + REMOVE + '(.+)$', 'i').exec(bare);
      const name = removal ? removal[1] : bare;
      const kind = Object.keys(REACTIONS).find(key => L.reactionName(key).test(name));
      if (!kind) return null;
      if (removal) return kind;
      return kind === 'like' && String(pressed) !== 'true' ? 'none' : kind;
    },
  };
  if (typeof window !== 'undefined') window.__blindfbLabels = L;
  if (typeof module !== 'undefined') module.exports = L;
})();
