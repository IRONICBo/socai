(function () {
  const PROFILE_PATH = /^\/in\/([^/?#]+)/i;
  const POST_PATH = /\/(?:posts\/|feed\/update\/urn:li:)/i;
  const SEARCH_PATH = /^\/search\/results\/(people|content|all)\/?/i;

  function cleanText(value, maxLength) {
    const raw = typeof value === 'string'
      ? value
      : value && (value.innerText || value.textContent) || '';
    const normalized = String(raw).replace(/\u00a0/g, ' ').replace(/[ \t]+/g, ' ')
      .replace(/\s*\n\s*/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
    return normalized.slice(0, Math.max(0, Number(maxLength || 12000)));
  }

  function visible(node) {
    if (!node || !node.getBoundingClientRect) return false;
    const rect = node.getBoundingClientRect();
    const style = window.getComputedStyle(node);
    return rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
  }

  function inViewport(node) {
    if (!visible(node)) return false;
    const rect = node.getBoundingClientRect();
    return rect.bottom > 0 && rect.top < window.innerHeight && rect.right > 0 && rect.left < window.innerWidth;
  }

  function firstNode(root, selectors) {
    if (!root || !root.querySelector) return null;
    const scope = root;
    for (const selector of selectors) {
      const node = scope.querySelector(selector);
      if (node) return node;
    }
    return null;
  }

  function firstText(root, selectors, maxLength) {
    return cleanText(firstNode(root, selectors), maxLength);
  }

  function firstVisibleNode(root, selectors) {
    if (!root || !root.querySelectorAll) return null;
    for (const selector of selectors) {
      const node = Array.from(root.querySelectorAll(selector)).find(visible);
      if (node) return node;
    }
    return null;
  }

  function linkedInUrl(raw) {
    if (!raw) return '';
    try {
      const url = new URL(raw, location.href);
      const host = url.hostname.toLowerCase();
      if (url.protocol !== 'https:' || !(host === 'linkedin.com' || host.endsWith('.linkedin.com'))) return '';
      url.hash = '';
      url.search = '';
      return url.href;
    } catch (_) {
      return '';
    }
  }

  function metaContent(name) {
    const node = document.querySelector(`meta[property="${name}"], meta[name="${name}"]`);
    return node && (node.getAttribute('content') || '').trim() || '';
  }

  function canonicalPageUrl() {
    const canonical = document.querySelector('link[rel="canonical"]');
    return linkedInUrl(canonical && canonical.href) || linkedInUrl(metaContent('og:url')) || linkedInUrl(location.href);
  }

  function profileIdFromUrl(raw) {
    try {
      const match = new URL(raw, location.href).pathname.match(PROFILE_PATH);
      return match ? decodeURIComponent(match[1]) : '';
    } catch (_) {
      return '';
    }
  }

  function activityId(raw, root) {
    const urn = root && (
      root.getAttribute('data-activity-urn') ||
      root.getAttribute('data-featured-activity-urn') ||
      root.getAttribute('data-urn') ||
      root.getAttribute('data-id')
    ) || '';
    const urnMatch = String(urn).match(/urn:li:(?:activity|ugcPost|share):(\d+)/i);
    if (urnMatch) return urnMatch[1];
    const value = String(raw || '');
    const urlMatch = value.match(/(?:activity-|urn:li:(?:activity|ugcPost|share):)(\d+)/i);
    return urlMatch ? urlMatch[1] : '';
  }

  function pageType() {
    const path = location.pathname;
    const search = path.match(SEARCH_PATH);
    if (search) return `search_${search[1].toLowerCase()}`;
    if (PROFILE_PATH.test(path)) return 'profile';
    if (POST_PATH.test(path)) return 'post';
    if (/^\/checkpoint(?:\/|$)/i.test(path)) return 'challenge';
    if (/^\/(?:authwall|uas\/login|login|signup)/i.test(path)) return 'login';
    if (/^\/feed\/?/i.test(path)) return 'feed';
    return 'unknown';
  }

  function challengeRequired() {
    if (/^\/checkpoint(?:\/|$)/i.test(location.pathname)) return true;
    const marker = firstVisibleNode(document, [
      '#challenge-dialog-modal-header',
      'iframe[title*="Security Verification"]',
      'iframe[title*="安全验证"]',
      '[data-test-id*="captcha"]',
      '[class*="challenge-dialog"]',
      '[class*="captcha"]',
    ]);
    return !!marker;
  }

  function rateLimited() {
    const marker = firstVisibleNode(document, [
      '[data-test-id*="rate-limit"]',
      '[class*="rate-limit"]',
      '[class*="commercial-use-limit"]',
      '[data-test-id*="commercial-use-limit"]',
    ]);
    if (marker) return true;
    const normalContent = postHasHydratedContent(postRoot()) || hasProfileContent() || searchResultNodes().length > 0;
    if (normalContent) return false;
    const error = firstVisibleNode(document, [
      'main [role="alert"]',
      'main .artdeco-inline-feedback--error',
      'main .error-container',
      'main h1',
    ]);
    return /too many requests|temporarily restricted|commercial use limit|unusual activity|请求过多|暂时受到限制|商业用途限制/i.test(cleanText(error, 2000));
  }

  function loginGatePresent() {
    return !!firstNode(document, [
      'form[action*="/uas/login"]',
      'form[data-id="sign-in-form"]',
      '.contextual-sign-in-modal',
      '.authwall-sign-in-form',
      '.authwall-join-form__title',
      'a[href*="/login"]',
    ]);
  }

  function loginRoute() {
    return /^\/(?:authwall|uas\/login|login|signup)/i.test(location.pathname);
  }

  function authenticated() {
    return !!firstNode(document, [
      '.global-nav__me',
      '[data-control-name="identity_welcome_message"]',
      'a[href*="/mynetwork/"]',
      'a[href*="/messaging/"]',
    ]);
  }

  function postRoot() {
    return firstNode(document, [
      'article[data-activity-urn]',
      'article[data-featured-activity-urn]',
      'main article.feed-shared-update-v2',
      'main [data-urn^="urn:li:activity:"]',
      'main article',
    ]);
  }

  function postHasHydratedContent(root) {
    if (!root) return false;
    const commentary = firstNode(root, [
      '[data-test-id="main-feed-activity-card__commentary"]',
      '.update-components-text',
      '[data-ad-preview="message"]',
      '.feed-shared-update-v2__description',
    ]);
    const actor = firstNode(root, [
      'a[data-tracking-control-name*="feed-actor-name"]',
      '.update-components-actor__name',
      '.feed-shared-actor__name',
    ]);
    const media = firstNode(root, [
      '.update-components-image img[src]',
      '.feed-shared-image img[src]',
      '.update-components-video video',
      '.feed-shared-external-video',
      '.document-s-container',
      '[data-test-id*="media"] img[src]',
    ]);
    return !!cleanText(commentary, 200) || !!cleanText(actor, 200) || !!media;
  }

  function hasProfileContent() {
    if (!PROFILE_PATH.test(location.pathname)) return false;
    return !!firstNode(document, [
      'main h1',
      '.top-card-layout__title',
      '.pv-text-details__left-panel',
    ]);
  }

  function pageState() {
    const bodyLength = cleanText(document.body, 200000).length;
    const root = postRoot();
    const hasPost = postHasHydratedContent(root);
    const hasProfile = hasProfileContent();
    const resultCount = searchResultNodes().length;
    const challenge = challengeRequired();
    const limited = rateLimited();
    const gate = loginGatePresent();
    const contentAvailable = hasPost || hasProfile || resultCount > 0;
    return {
      ok: !challenge && !limited && !loginRoute(),
      site: 'linkedin',
      url: location.href,
      canonical_url: canonicalPageUrl(),
      title: document.title || '',
      page_type: pageType(),
      ready_state: document.readyState,
      body_text_len: bodyLength,
      authenticated: authenticated(),
      login_required: loginRoute(),
      login_gate_present: gate,
      challenge_required: challenge,
      rate_limited: limited,
      content_available: contentAvailable,
      result_count: resultCount,
      hydrated: document.readyState !== 'loading' && (bodyLength > 20 || challenge || limited),
      blank_or_throttled: document.readyState === 'loading' || (bodyLength < 20 && !challenge && !limited),
    };
  }

  function searchResultNodes() {
    if (!SEARCH_PATH.test(location.pathname)) return [];
    const selectors = [
      'main li.reusable-search__result-container',
      'main .entity-result',
      'main [data-chameleon-result-urn]',
      'main [data-view-name="search-entity-result-universal-template"]',
    ];
    const nodes = [];
    const seen = new Set();
    for (const selector of selectors) {
      for (const node of document.querySelectorAll(selector)) {
        const card = node.closest('li.reusable-search__result-container, .entity-result, [data-chameleon-result-urn]') || node;
        if (seen.has(card) || card.closest('header, nav, aside')) continue;
        const link = card.querySelector('a[href*="/in/"], a[href*="/posts/"], a[href*="/feed/update/urn:li:"]');
        if (!link) continue;
        seen.add(card);
        nodes.push(card);
      }
    }
    if (nodes.length) return nodes;

    for (const link of document.querySelectorAll('main a[href*="/in/"], main a[href*="/posts/"], main a[href*="/feed/update/urn:li:"]')) {
      const card = link.closest('li, article, [data-view-name], [data-urn]') || link;
      if (seen.has(card) || card.closest('header, nav, aside')) continue;
      seen.add(card);
      nodes.push(card);
    }
    return nodes;
  }

  function resultLink(card, expectedType) {
    const profiles = Array.from(card.querySelectorAll('a[href*="/in/"]'));
    const posts = Array.from(card.querySelectorAll('a[href*="/posts/"], a[href*="/feed/update/urn:li:"]'));
    if (expectedType === 'people') return profiles[0] || null;
    if (expectedType === 'content') return posts[0] || null;
    return posts[0] || profiles[0] || null;
  }

  function normalizeResultType(value) {
    const kind = String(value || 'all').trim().toLowerCase();
    return ['people', 'content', 'all'].includes(kind) ? kind : 'all';
  }

  function searchResults(arg) {
    const input = arg || {};
    const limit = Math.min(100, Math.max(1, Number(input.limit || 25)));
    const expectedType = normalizeResultType(input.result_type);
    const viewportOnly = !!input.viewport_only;
    const output = [];
    const seen = new Set();

    for (const card of searchResultNodes()) {
      if (viewportOnly && !inViewport(card)) continue;
      const link = resultLink(card, expectedType);
      const url = linkedInUrl(link && (link.href || link.getAttribute('href')) || '');
      if (!url) continue;
      const kind = PROFILE_PATH.test(new URL(url).pathname) ? 'profile' : 'post';
      if (expectedType === 'people' && kind !== 'profile') continue;
      if (expectedType === 'content' && kind !== 'post') continue;
      const id = kind === 'profile' ? profileIdFromUrl(url) : activityId(url, card);
      const key = id ? `${kind}:${id}` : url;
      if (seen.has(key)) continue;
      seen.add(key);

      const title = firstText(card, [
        '.entity-result__title-text span[aria-hidden="true"]',
        '[data-anonymize="person-name"]',
        '[data-view-name="search-entity-result-universal-template"] h3',
        'h3',
      ], 500) || cleanText(link, 500);
      const subtitle = firstText(card, [
        '.entity-result__primary-subtitle',
        '.entity-result__secondary-subtitle',
        '[data-anonymize="headline"]',
        '.t-14.t-black.t-normal',
      ], 1000);
      const snippet = firstText(card, [
        '.entity-result__summary',
        '.entity-result__content-summary',
        '.update-components-text',
        '[data-ad-preview="message"]',
      ], 3000);
      output.push({
        kind,
        id,
        url,
        title,
        subtitle,
        snippet,
        position: output.length,
      });
      if (output.length >= limit) break;
    }
    return output;
  }

  function normalizedQuery(value) {
    return String(value || '').replace(/\+/g, ' ').replace(/\s+/g, ' ').trim().toLowerCase();
  }

  function searchState(arg) {
    const input = arg || {};
    const match = location.pathname.match(SEARCH_PATH);
    const actualType = match ? match[1].toLowerCase() : '';
    const expectedType = normalizeResultType(input.result_type);
    const url = new URL(location.href);
    const actualQuery = url.searchParams.get('keywords') || url.searchParams.get('q') || '';
    const expectedQuery = String(input.query || '').trim();
    const results = searchResults({ limit: 100, result_type: expectedType });
    const mainText = cleanText(document.querySelector('main') || document.body, 10000);
    const emptyMarker = firstVisibleNode(document, [
      'main .search-reusables__no-results',
      'main [data-test-id*="no-results"]',
      'main .artdeco-empty-state',
    ]);
    const empty = results.length === 0 && (
      !!emptyMarker || /no results found|try shortening or rephrasing your search|未找到结果|没有找到结果/i.test(mainText)
    );
    const challenge = challengeRequired();
    const limited = rateLimited();
    const needsLogin = loginRoute();
    const routeMatches = !!match && (expectedType === 'all' || actualType === expectedType);
    const queryMatches = !expectedQuery || normalizedQuery(actualQuery) === normalizedQuery(expectedQuery);
    const hydrated = document.readyState !== 'loading' && (results.length > 0 || empty || challenge || limited || needsLogin);
    let error = '';
    if (challenge) error = 'challenge_required';
    else if (limited) error = 'rate_limited';
    else if (needsLogin) error = 'login_required';
    else if (!match) error = 'wrong_search_route';
    else if (!routeMatches) error = 'result_type_mismatch';
    else if (!queryMatches) error = 'query_mismatch';
    else if (!hydrated) error = 'not_hydrated';
    return {
      ok: !error,
      error,
      url: location.href,
      on_search_page: !!match,
      actual_result_type: actualType,
      expected_result_type: expectedType,
      actual_query: actualQuery,
      expected_query: expectedQuery,
      valid_transition: !!match && routeMatches && queryMatches,
      hydrated,
      empty,
      result_count: results.length,
      login_required: needsLogin,
      challenge_required: challenge,
      rate_limited: limited,
    };
  }

  async function scrollResults(arg) {
    if (!SEARCH_PATH.test(location.pathname)) {
      return { ok: false, error: 'wrong_search_route', url: location.href };
    }
    const before = { scroll_y: window.scrollY, result_count: searchResultNodes().length };
    const input = arg || {};
    if (input.to_top) window.scrollTo({ top: 0, behavior: 'auto' });
    else {
      const step = Math.max(320, Math.min(900, Math.floor(window.innerHeight * 0.8)));
      window.scrollBy({ top: input.nudge_up ? -Math.floor(step / 2) : step, behavior: 'auto' });
    }
    await new Promise((resolve) => setTimeout(resolve, 350));
    return {
      ok: true,
      url: location.href,
      before,
      after: { scroll_y: window.scrollY, result_count: searchResultNodes().length },
    };
  }

  function sectionByAnchor(id) {
    const anchor = document.getElementById(id);
    return anchor && (anchor.closest('section') || anchor.parentElement && anchor.parentElement.closest('section')) || null;
  }

  function sectionByHeading(pattern) {
    for (const section of document.querySelectorAll('main section')) {
      const heading = section.querySelector(':scope > h2, :scope > header h2');
      if (heading && pattern.test(cleanText(heading, 500))) return section;
    }
    return null;
  }

  function profileValue(value) {
    const normalized = cleanText(value, 2000).replace(/\bundefined\b/gi, '').trim();
    if (!normalized || /^[\s*•_-]+$/.test(normalized)) return '';
    return normalized;
  }

  function experienceItems(section) {
    if (!section) return [];
    for (const selector of [
      'ul.visible-list > li.profile-section-card',
      '.pvs-list__paged-list-item',
      'li.artdeco-list__item',
      '.experience-item',
    ]) {
      const items = Array.from(section.querySelectorAll(selector));
      if (items.length) return items;
    }
    return [];
  }

  function experienceIsCurrent(item) {
    const date = firstText(item, [
      '.pvs-entity__caption-wrapper',
      '[class*="date-range"]',
      '.date-range',
      'time',
    ], 1000);
    return /\bpresent\b|\bcurrent(?:ly)?\b|至今|目前/i.test(date);
  }

  function experienceRole(item) {
    if (!item) return { title: '', organization: '', text: '' };
    const title = profileValue(firstText(item, [
      '.t-bold span[aria-hidden="true"]',
      '.experience-item__title',
      'h4',
    ], 500));
    const organization = profileValue(firstText(item, [
      '.t-normal span[aria-hidden="true"]',
      '.experience-item__subtitle',
      'h3',
    ], 500));
    return {
      title,
      organization,
      text: [title, organization].filter(Boolean).join(' — '),
    };
  }

  function profileDetail() {
    const state = pageState();
    if (state.challenge_required) return { ok: false, error: 'challenge_required', url: location.href };
    if (state.rate_limited) return { ok: false, error: 'rate_limited', url: location.href };
    if (state.login_required) return { ok: false, error: 'login_required', url: location.href };
    if (!PROFILE_PATH.test(location.pathname)) return { ok: false, error: 'wrong_profile_route', url: location.href };

    const main = document.querySelector('main') || document;
    const top = firstNode(main, ['.pv-text-details__left-panel', '.top-card-layout__card', 'section:first-of-type']) || main;
    const name = firstText(top, ['h1', '.top-card-layout__title'], 500) || firstText(main, ['h1'], 500);
    const headline = firstText(top, [
      '.text-body-medium.break-words',
      '.top-card-layout__headline',
      '[data-anonymize="headline"]',
    ], 1200);
    const locationText = firstText(top, [
      '.profile-info-subheader > span:first-child',
      '.text-body-small.inline.t-black--light.break-words',
      '.top-card-layout__first-subline',
      '[data-anonymize="location"]',
    ], 500);
    const aboutSection = sectionByAnchor('about') || firstNode(main, ['section.summary', 'section[data-section="summary"]']);
    const aboutContent = firstNode(aboutSection, ['.core-section-container__content > div:first-child']) || aboutSection;
    const experienceSection = sectionByAnchor('experience') ||
      sectionByHeading(/experience|工作经历|职业经历/i) || firstNode(main, ['section.experience']);
    const roles = experienceItems(experienceSection);
    const latestExperience = roles[0] || null;
    const currentExperience = roles.find(experienceIsCurrent) || null;
    const latestRole = experienceRole(latestExperience);
    const currentRole = currentExperience ? experienceRole(currentExperience) : null;
    const url = canonicalPageUrl();
    const profileId = profileIdFromUrl(url || location.href);
    const about = cleanText(aboutContent, 6000).replace(/^about\s*/i, '')
      .replace(/\s*(?:see more|展开)\s*$/i, '').trim();
    if (!name) {
      return { ok: false, error: 'profile_not_hydrated', url: location.href, profile_id: profileId };
    }
    return {
      ok: true,
      profile_id: profileId,
      url,
      name,
      headline,
      location: locationText,
      about,
      current_role: currentRole,
      latest_role: { ...latestRole, is_current: !!currentExperience && currentExperience === latestExperience },
      login_gate_present: state.login_gate_present,
    };
  }

  function postDetail() {
    const state = pageState();
    if (state.challenge_required) return { ok: false, error: 'challenge_required', url: location.href };
    if (state.rate_limited) return { ok: false, error: 'rate_limited', url: location.href };
    const root = postRoot();
    if (!root) {
      return { ok: false, error: state.login_required ? 'login_required' : 'post_not_hydrated', url: location.href };
    }
    const body = firstText(root, [
      '[data-test-id="main-feed-activity-card__commentary"]',
      '.update-components-text',
      '[data-ad-preview="message"]',
      '.feed-shared-update-v2__description',
      '.attributed-text-segment-list__container',
    ], 20000);
    const authorLink = firstNode(root, [
      'a[data-tracking-control-name*="feed-actor-name"]',
      '.update-components-actor__name a',
      '.feed-shared-actor__name a',
      'a[href*="/in/"]',
    ]);
    const authorUrl = linkedInUrl(authorLink && (authorLink.href || authorLink.getAttribute('href')) || '');
    const authorName = firstText(root, [
      'a[data-tracking-control-name*="feed-actor-name"]',
      '.update-components-actor__name span[aria-hidden="true"]',
      '.feed-shared-actor__name span[aria-hidden="true"]',
    ], 500) || cleanText(authorLink, 500);
    const published = firstText(root, [
      'time',
      '.update-components-actor__sub-description span[aria-hidden="true"]',
      '.feed-shared-actor__sub-description',
    ], 500);
    const reactionsNode = firstNode(root, [
      '[data-id="social-actions__reactions"]',
      '.social-details-social-counts__reactions-count',
    ]);
    const commentsNode = firstNode(root, [
      '[data-id="social-actions__comments"]',
      '.social-details-social-counts__comments',
    ]);
    const mediaNode = firstNode(root, [
      '.update-components-image img[src]',
      '.feed-shared-image img[src]',
      '.update-components-video video',
      '.feed-shared-external-video',
      '.document-s-container',
      '[data-test-id*="media"] img[src]',
    ]);
    const url = canonicalPageUrl();
    const postId = activityId(url || location.href, root);
    const rootText = cleanText(root, 5000);
    if (/post (?:is )?no longer available|content (?:is )?unavailable|动态已不可用|内容不可用/i.test(rootText)) {
      return { ok: false, error: 'post_unavailable', url: location.href, post_id: postId };
    }
    if (!postHasHydratedContent(root)) {
      return { ok: false, error: 'post_not_hydrated', url: location.href, post_id: postId };
    }
    return {
      ok: true,
      post_id: postId,
      activity_urn: root.getAttribute('data-activity-urn') || root.getAttribute('data-featured-activity-urn') || root.getAttribute('data-urn') || '',
      url,
      author: {
        name: authorName,
        profile_id: profileIdFromUrl(authorUrl),
        url: authorUrl,
      },
      text: body,
      has_media: !!mediaNode,
      published_at: published,
      engagement: {
        reactions: reactionsNode && (reactionsNode.getAttribute('data-num-reactions') || cleanText(reactionsNode, 500)) || '',
        comments: commentsNode && (commentsNode.getAttribute('data-num-comments') || cleanText(commentsNode, 500)) || '',
      },
      login_gate_present: state.login_gate_present,
    };
  }

  function commentNodes(root) {
    const scope = root && root.querySelectorAll ? root : document;
    const selectors = [
      'section.comment',
      '.comments-comment-item',
      '[data-id^="urn:li:comment"]',
      '[data-urn^="urn:li:comment"]',
    ];
    const output = [];
    const seen = new Set();
    for (const selector of selectors) {
      for (const node of scope.querySelectorAll(selector)) {
        const item = node.closest('section.comment, .comments-comment-item, [data-id^="urn:li:comment"], [data-urn^="urn:li:comment"]') || node;
        if (seen.has(item)) continue;
        seen.add(item);
        output.push(item);
      }
    }
    return output;
  }

  function comments(arg) {
    const root = postRoot();
    if (!root) return [];
    const limit = Math.min(100, Math.max(1, Number(arg && arg.limit || 30)));
    const output = [];
    const seen = new Set();
    for (const node of commentNodes(root)) {
      const authorLink = firstNode(node, [
        'a[data-tracking-control-name*="comment_actor-name"]',
        '.comments-post-meta__name-text a',
        '.comments-comment-meta__description-container a[href*="/in/"]',
        'a[href*="/in/"]',
      ]);
      const authorUrl = linkedInUrl(authorLink && (authorLink.href || authorLink.getAttribute('href')) || '');
      const author = cleanText(authorLink, 500) || firstText(node, [
        '.comments-post-meta__name-text',
        '.comments-comment-meta__description-title',
      ], 500);
      const body = firstText(node, [
        'p.comment__text',
        '.comments-comment-item__main-content',
        '.comments-comment-item-content-body',
        '[data-test-id="comment-content"]',
      ], 6000);
      const published = firstText(node, [
        '.comment__duration-since',
        '.comments-comment-meta__data',
        'time',
      ], 500);
      const reactions = firstText(node, [
        '.comment__reactions-count:not(.hidden)',
        '.comments-comment-social-bar__reactions-count',
      ], 500);
      if (!body) continue;
      const key = `${authorUrl}|${author}|${published}|${body}`;
      if (seen.has(key)) continue;
      seen.add(key);
      output.push({
        comment_id: node.getAttribute('data-id') || node.getAttribute('data-urn') || '',
        author,
        author_id: profileIdFromUrl(authorUrl),
        author_url: authorUrl,
        text: body,
        published_at: published,
        reactions,
        position: output.length,
      });
      if (output.length >= limit) break;
    }
    return output;
  }

  window.SocaiLinkedInPageScripts = Object.freeze({
    pageState,
    searchState,
    searchResults,
    scrollResults,
    profileDetail,
    postDetail,
    comments,
  });
})();
