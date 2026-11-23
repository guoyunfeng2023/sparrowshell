(function () {
  function savedTheme(key) {
    try {
      var preferences = JSON.parse(localStorage.getItem(key) || '{}');
      return preferences.theme === 'light' || preferences.theme === 'dark' ? preferences.theme : null;
    } catch (_) {
      return null;
    }
  }

  var theme = savedTheme('sparrowshell.desktop.development.preferences')
    || savedTheme('sparrowshell.desktop.preferences')
    || 'dark';
  var background = theme === 'light' ? '#ffffff' : '#0d1012';
  var root = document.documentElement;
  root.dataset.theme = theme;
  root.style.backgroundColor = background;
  root.style.colorScheme = theme;
  var themeColor = document.querySelector('meta[name="theme-color"]');
  if (themeColor) themeColor.setAttribute('content', background);
})();
