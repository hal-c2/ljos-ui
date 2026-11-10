// Stands in for Hangman, the game the example registers from C++ in its own
// module: a word from the example's dict.txt, the letters asked for, and the
// counts the views show. Written from what the example's QML asks of it and
// the game's own help text, not from the C++.
//
// The word is in capitals, as the keys that ask for its letters are. The
// dictionary is read when the object is complete, as the C++ reads it off
// the views' thread, so the splash screen shows until the first word comes.
//
// DECISION: what the views do not show is chosen here. Each letter the word
// lacks is a mistake, and so are a wrong guess of the whole word and Reveal,
// which shows one letter not yet shown. The score is the words guessed. The
// game starts with 100 vowels, as the documentation's picture of a game
// shows it; nothing is kept between runs.
import QtQml

QtObject {
    id: game

    property string word: ""
    property string lettersOwned: ""
    readonly property string vowels: "AEIOU"
    readonly property string consonants: "BCDFGHJKLMNPQRSTVWXYZ"
    property int errorCount: 0
    property bool vowelsUnlocked: false
    property int vowelsAvailable: 100
    property int wordsGiven: 0
    property int wordsGuessedCorrectly: 0
    property int score: wordsGuessedCorrectly

    property var dictionary: []

    function reset() {
        lettersOwned = ""
        errorCount = 0
        if (dictionary.length > 0)
            word = dictionary[Math.floor(Math.random() * dictionary.length)]
    }

    function reveal() {
        const hidden = []
        for (let i = 0; i < word.length; ++i) {
            const letter = word.charAt(i)
            if (lettersOwned.indexOf(letter) < 0 && hidden.indexOf(letter) < 0)
                hidden.push(letter)
        }
        if (hidden.length === 0)
            return
        lettersOwned += hidden[Math.floor(Math.random() * hidden.length)]
        errorCount += 1
    }

    function gameOverReveal() {
        ownAll()
    }

    function requestLetter(letterString) {
        const letter = letterString.toUpperCase()
        if (letter.length !== 1 || lettersOwned.indexOf(letter) >= 0)
            return
        lettersOwned += letter
        if (word.indexOf(letter) < 0)
            errorCount += 1
    }

    function guessWord(guess) {
        if (guess.toUpperCase() === word)
            ownAll()
        else
            errorCount += 1
    }

    function isVowel(letter) {
        return letter.length === 1 && vowels.indexOf(letter.toUpperCase()) >= 0
    }

    function setVowelsAvailable(count) { vowelsAvailable = count }
    function setWordsGiven(count) { wordsGiven = count }
    function setWordsGuessedCorrectly(count) { wordsGuessedCorrectly = count }
    function setScore(value) { score = value }

    function ownAll() {
        let owned = lettersOwned
        for (let i = 0; i < word.length; ++i) {
            if (owned.indexOf(word.charAt(i)) < 0)
                owned += word.charAt(i)
        }
        lettersOwned = owned
    }

    Component.onCompleted: {
        const dictionaryUrl = Qt.resolvedUrl("../../../../qtdoc/examples/demos/hangman/dict.txt")
        const request = new XMLHttpRequest()
        request.onreadystatechange = function() {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            const words = []
            for (const line of request.responseText.split("\n")) {
                const entry = line.trim().toUpperCase()
                if (entry.length > 0)
                    words.push(entry)
            }
            if (words.length === 0) {
                console.warn("Hangman: no words in the dictionary " + dictionaryUrl)
                return
            }
            game.dictionary = words
            game.reset()
        }
        request.open("GET", dictionaryUrl)
        request.send()
    }
}
