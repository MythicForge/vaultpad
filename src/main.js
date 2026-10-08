import { mount } from 'svelte';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import './app.css';
import App from './App.svelte';
import Sticky from './lib/Sticky.svelte';
import { stickyIdFromLabel } from './lib/api.js';

// Every window loads this page; sticky note windows are told apart by label.
const stickyId = isTauri() ? stickyIdFromLabel(getCurrentWindow().label) : null;
const target = document.getElementById('app');

export default stickyId ? mount(Sticky, { target, props: { id: stickyId } }) : mount(App, { target });
