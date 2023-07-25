import { answer } from "./answer";
import './bar'
function render() {
	document.getElementById(
		"root"
	).innerHTML = `the answer to the universe is ${answer}`;
}
render();
