// Phosphor-Icons (regular), wie vom Nocturne-Designsystem vorgegeben. Nur benötigte Icons importieren.
import arrowLeft from "@phosphor-icons/core/assets/regular/arrow-left.svg?raw";
import caretDown from "@phosphor-icons/core/assets/regular/caret-down.svg?raw";
import caretRight from "@phosphor-icons/core/assets/regular/caret-right.svg?raw";
import folderOpen from "@phosphor-icons/core/assets/regular/folder-open.svg?raw";
import folderSimple from "@phosphor-icons/core/assets/regular/folder-simple.svg?raw";
import folderSimplePlus from "@phosphor-icons/core/assets/regular/folder-simple-plus.svg?raw";
import squaresFour from "@phosphor-icons/core/assets/regular/squares-four.svg?raw";
import user from "@phosphor-icons/core/assets/regular/user.svg?raw";
import cloud from "@phosphor-icons/core/assets/regular/cloud.svg?raw";
import cloudArrowDown from "@phosphor-icons/core/assets/regular/cloud-arrow-down.svg?raw";
import cloudCheck from "@phosphor-icons/core/assets/regular/cloud-check.svg?raw";
import cloudSlash from "@phosphor-icons/core/assets/regular/cloud-slash.svg?raw";
import cloudWarning from "@phosphor-icons/core/assets/regular/cloud-warning.svg?raw";
import googleDriveLogo from "@phosphor-icons/core/assets/regular/google-drive-logo.svg?raw";
import linkBreak from "@phosphor-icons/core/assets/regular/link-break.svg?raw";
import arrowsClockwise from "@phosphor-icons/core/assets/regular/arrows-clockwise.svg?raw";
import check from "@phosphor-icons/core/assets/regular/check.svg?raw";
import clockCountdown from "@phosphor-icons/core/assets/regular/clock-countdown.svg?raw";
import copy from "@phosphor-icons/core/assets/regular/copy.svg?raw";
import diceFive from "@phosphor-icons/core/assets/regular/dice-five.svg?raw";
import eye from "@phosphor-icons/core/assets/regular/eye.svg?raw";
import eyeSlash from "@phosphor-icons/core/assets/regular/eye-slash.svg?raw";
import gearSix from "@phosphor-icons/core/assets/regular/gear-six.svg?raw";
import key from "@phosphor-icons/core/assets/regular/key.svg?raw";
import lockSimple from "@phosphor-icons/core/assets/regular/lock-simple.svg?raw";
import magnifyingGlass from "@phosphor-icons/core/assets/regular/magnifying-glass.svg?raw";
import password from "@phosphor-icons/core/assets/regular/password.svg?raw";
import pencilSimple from "@phosphor-icons/core/assets/regular/pencil-simple.svg?raw";
import plus from "@phosphor-icons/core/assets/regular/plus.svg?raw";
import robot from "@phosphor-icons/core/assets/regular/robot.svg?raw";
import shieldCheck from "@phosphor-icons/core/assets/regular/shield-check.svg?raw";
import trash from "@phosphor-icons/core/assets/regular/trash.svg?raw";
import warningCircle from "@phosphor-icons/core/assets/regular/warning-circle.svg?raw";
import x from "@phosphor-icons/core/assets/regular/x.svg?raw";

export const icons = {
  "cloud": cloud,
  "cloud-arrow-down": cloudArrowDown,
  "cloud-check": cloudCheck,
  "cloud-slash": cloudSlash,
  "cloud-warning": cloudWarning,
  "google-drive-logo": googleDriveLogo,
  "link-break": linkBreak,
  "arrow-left": arrowLeft,
  "caret-down": caretDown,
  "caret-right": caretRight,
  "folder-open": folderOpen,
  "folder-simple": folderSimple,
  "folder-simple-plus": folderSimplePlus,
  "squares-four": squaresFour,
  "user": user,
  "arrows-clockwise": arrowsClockwise,
  check,
  "clock-countdown": clockCountdown,
  copy,
  "dice-five": diceFive,
  eye,
  "eye-slash": eyeSlash,
  "gear-six": gearSix,
  key,
  "lock-simple": lockSimple,
  "magnifying-glass": magnifyingGlass,
  password,
  "pencil-simple": pencilSimple,
  plus,
  robot,
  "shield-check": shieldCheck,
  trash,
  "warning-circle": warningCircle,
  x,
} as const;

export type IconName = keyof typeof icons;
