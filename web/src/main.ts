import '@fontsource-variable/ibm-plex-sans/wght.css';
import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { theme } from './lib/theme.svelte';

theme.apply();

const target = document.getElementById('app');
if (!target) throw new Error('#app is missing');

export default mount(App, { target });
