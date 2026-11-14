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
    // "See original (Russian)": the language in brackets is optional.
    commentAction: exact(`Like|Reply|Edited|See translation|See original(?: [(][^)]+[)])?|Hide translation|Rate this translation|Translation unavailable|Thích|Trả lời|Xem bản dịch|Xem bản gốc|J${A}aime|Répondre|Modifié|Voir la traduction|Voir l${A}original(?: [(][^)]+[)])?|Masquer la traduction`),
    // Facebook's own translation toggle on a post, a caption or a comment.
    seeTranslation: exact('See translation|Voir la traduction|Xem bản dịch'),
    hideTranslation: exact(`See originals?(?: [(][^)]+[)])?|Hide translation|Voir l${A}original(?: [(][^)]+[)])?|Masquer la traduction|Xem bản gốc|Ẩn bản dịch`),
    // Badges on a comment's author line.
    commentBadge: exact('(?:by )?author|Top fan|Follow|Auteur|Fan assidu|Super fan|Suivre|·'),
    // A comment's own Like button, liked or not.
    commentLike: exact(`Like|Remove Like|Unlike|J${A}aime|${REMOVE}J${A}aime|Je n${A}aime plus`),
    reply: exact('Reply|Trả lời|Répondre'),
    replyStart: /^(?:Reply|Trả lời|Répondre)\b/i,
    // Buttons that load a comment's hidden replies: "View replies", "View 3 replies",
    // "View more replies". Never "Hide replies" — that would collapse them again.
    // Czech and Slovak (issue #30, Michal's languages, already taught to the audience
    // picker in issue #20 but never to this pattern, so his replies never loaded):
    // "Zobrazit odpovědi / 1 odpověď / další odpovědi", "Zobraziť odpovede / ďalšie
    // odpovede"; the noun stems cover every count declension (odpověď/odpovědi/odpovědí).
    viewReplies: exact(`View(?: more| previous)?(?: ${COUNT})? replies|View 1 reply|Afficher(?: les| plus de)?(?: ${COUNT})? réponses|Voir(?: les| plus de)?(?: ${COUNT})? réponses|Xem(?: thêm)?(?: ${COUNT})?(?: câu)? trả lời|Zobrazit(?: další| více| předchozí| všechny)?(?: ${COUNT})? odpově(?:ď|di|dí)|Zobraziť(?: ďalšie| viac| predchádzajúce| všetky)?(?: ${COUNT})? odpove(?:ď|de|dí)`),
    // Buttons that load more top-level comments: "View more comments", "View previous
    // comments". read() never pressed these, so a post's list stopped at Facebook's
    // first batch while its count (which includes replies) kept counting (issue #30).
    // Never a sort label ("Most relevant") and never "Hide comments".
    viewMoreComments: exact(`View(?: more| previous)?(?: ${COUNT})? comments|View ${COUNT} more comments|Voir(?: plus de| les)?(?: ${COUNT})? commentaires|Afficher(?: plus de| les)?(?: ${COUNT})? commentaires|Xem(?: thêm)?(?: ${COUNT})? bình luận|Xem các bình luận trước(?: đó)?|Zobrazit(?: další| více| předchozí)?(?: ${COUNT})? komentář(?:e|ů)?|Zobraziť(?: ďalšie| viac| predchádzajúce)?(?: ${COUNT})? komentár(?:e|ov)?`),
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
    // "See more" in any language: a short button right after text that Facebook cut
    // with "…" (its words are known only for the languages above).
    cutShort(element, text) {
      const parent = element && element.parentElement;
      if (!text || text.length > 40 || !parent || typeof parent.innerText !== 'string') return false;
      const whole = parent.innerText.trim();
      return whole.endsWith(text) && /(?:…|\.\.\.)$/.test(whole.slice(0, -text.length).trim());
    },
    // Text as the lines a reader sees, for the elements keep() accepts. Facebook splits one
    // line over inline spans and links (a mention, a hashtag, a translated phrase), so a line
    // ends only where the nearest non-inline box changes or at a <br>; read one text node per
    // line and a word could break in two. Emoji pictures (img alt) join the words around them.
    textLines(scope, keep) {
      const EMOJI = /^[\p{Extended_Pictographic}\p{Emoji_Component}]+$/u;
      const box = element => {
        if (typeof getComputedStyle !== 'function') return null;
        for (let e = element; e && e !== scope; e = e.parentElement) if (!/^(?:inline|contents)$/.test(getComputedStyle(e).display)) return e;
        return scope;
      };
      const walker = document.createTreeWalker(scope, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
      const rows = [];
      let node, last = null;
      while ((node = walker.nextNode())) {
        const picture = node.nodeType === 1;
        if (picture && node.tagName === 'BR') { last = null; continue; }
        if (picture && !(node.tagName === 'IMG' && EMOJI.test(node.alt))) continue;
        const element = picture ? node : node.parentElement;
        if (!element || !keep(element)) continue;
        const block = element.closest('[dir=auto]'), line = box(element);
        const text = picture ? node.alt : node.nodeValue || '';
        if (last && ((block && last.block === block && (picture || last.picture)) || (line && last.line === line))) last.text += text;
        else rows.push(last = {block, line, text});
        last.picture = picture;
      }
      return rows.map(row => row.text);
    },
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
