<script setup>
import { onBeforeUnmount, onMounted, ref } from "vue";
import UiIcon from "./common/UiIcon.vue";

const slogan = "小胖出品，必属废品";
const HOLD = 2400;
const leave = ref(false);
const gone = ref(false);
let holdTimer, goneTimer;
function dismiss() {
    if (leave.value) return;
    leave.value = true;
    clearTimeout(holdTimer);
    goneTimer = setTimeout(() => { gone.value = true; }, 520);
}
onMounted(() => { holdTimer = setTimeout(dismiss, HOLD); });
onBeforeUnmount(() => { clearTimeout(holdTimer); clearTimeout(goneTimer); });
</script>

<template>
    <div v-if="!gone" class="splash" :class="{ leaving: leave }" @click="dismiss">
        <div class="splash-stage">
            <span class="splash-badge"><UiIcon name="ticket" /></span>
            <p class="splash-slogan" :aria-label="slogan"><span v-for="(char, index) in [...slogan]" :key="index" class="splash-char" aria-hidden="true" :style="{ animationDelay: `${120 + index * 90}ms` }">{{ char }}</span></p>
            <span class="splash-rule"></span>
            <p class="splash-note">正在准备工作台</p>
        </div>
        <p class="splash-skip">点击任意位置跳过</p>
    </div>
</template>

<style scoped>
.splash { position: fixed; inset: 0; z-index: 9999; display: flex; flex-direction: column; align-items: center; justify-content: center; background: radial-gradient(circle at 50% 40%, var(--accent-tint), var(--page-bg) 68%); cursor: pointer; user-select: none; animation: splash-in .3s ease both; }
.splash.leaving { animation: splash-out .52s cubic-bezier(.4, 0, .2, 1) forwards; }
.splash-stage { display: flex; flex-direction: column; align-items: center; gap: 22px; }
.splash-badge { width: 62px; height: 68px; border-radius: 18px; background: var(--accent); color: var(--accent-contrast); display: inline-flex; align-items: center; justify-content: center; box-shadow: 0 18px 38px var(--shadow); animation: splash-badge .7s cubic-bezier(.2, .9, .25, 1.3) both; }
.splash-badge .ui-icon { width: 34px; height: 34px; }
.splash-slogan { display: flex; font-size: 40px; font-weight: 800; letter-spacing: 2px; color: var(--ink); line-height: 1.25; }
.splash-char { display: inline-block; animation: splash-char .52s cubic-bezier(.2, .85, .25, 1.15) both; }
.splash-char:nth-child(5) { color: var(--muted); }
.splash-rule { width: 190px; height: 3px; border-radius: 3px; background: linear-gradient(90deg, transparent, var(--accent), transparent); animation: splash-rule .8s ease 1.05s both; }
.splash-note { font-size: 12px; letter-spacing: 3.4px; color: var(--muted); animation: splash-fade .6s ease 1.3s both; }
.splash-skip { position: absolute; bottom: 42px; font-size: 11px; letter-spacing: 1.2px; color: var(--muted); animation: splash-skip .6s ease 1.6s both; }
@keyframes splash-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes splash-out { to { opacity: 0; transform: scale(1.05); } }
@keyframes splash-badge { from { opacity: 0; transform: rotate(-16deg) scale(.45); } to { opacity: 1; transform: rotate(-7deg) scale(1); } }
@keyframes splash-char { from { opacity: 0; transform: translateY(22px) rotate(-10deg) scale(.6); filter: blur(8px); } 65% { opacity: 1; transform: translateY(0) rotate(3deg) scale(1.08); filter: blur(0); } to { opacity: 1; transform: translateY(0) rotate(0) scale(1); } }
@keyframes splash-rule { from { opacity: 0; transform: scaleX(0); } to { opacity: 1; transform: scaleX(1); } }
@keyframes splash-fade { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }
@keyframes splash-skip { from { opacity: 0; transform: translateY(8px); } to { opacity: .7; transform: translateY(0); } }
@media (max-width: 620px) { .splash-slogan { font-size: 26px; } }
@media (prefers-reduced-motion: reduce) { .splash, .splash-badge, .splash-char, .splash-rule, .splash-note, .splash-skip { animation-duration: .01ms; animation-delay: 0ms; } }
</style>
