// Census page: single gliders. One viewer per rule.
import '../essay/essay.css';
import './census.css';
import { mountCensusViewer } from './viewer';
import { hatA, hatB, hatC, spectre } from './singles-objects';

mountCensusViewer(document.getElementById('cv-hatA')!, hatA);
mountCensusViewer(document.getElementById('cv-hatB')!, hatB);
mountCensusViewer(document.getElementById('cv-hatC')!, hatC);
mountCensusViewer(document.getElementById('cv-spectre')!, spectre);
